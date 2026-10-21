/// 技能库页（§11.3）：搜索 · 来源过滤 · 视图切换 · 扫描按钮；
/// 卡片网格 / 列表（>300 行走自建虚拟滚动）；
/// agent 角标常驻可见，点击即部署 / 撤销；
/// 两级删除语义：「取消部署」（正本不动）与「删除 Skill」（正本入回收站，二次确认）。

import { useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { LayoutGrid, List, RefreshCw, Search, Unlink, Trash2 } from "lucide-react";
import {
  ipcError,
  isReadOnlySkill,
  listenScanEvents,
  scanStart,
  scanStatus,
  type ScanProgress,
  type ScanReport,
  type SkillMeta,
  type ToolDto,
} from "../api/ipc";
import {
  queryKeys,
  useDeployApply,
  useDeployRevoke,
  useInstallLocal,
  useLibrary,
  useRemoveSkill,
  useTargets,
  useTools,
} from "../api/queries";
import VirtualList from "../components/VirtualList";
import { useToast } from "../components/Toast";
import { Badge, Button, EmptyState, Modal, Skeleton } from "../components/ui";
import { useQueryClient } from "@tanstack/react-query";
import SkillDetailPage from "./SkillDetailPage";

type OriginFilter = "all" | "local" | "git" | "archive";
type ViewMode = "grid" | "list";
type RescanState = { phase: "idle" } | { phase: "running"; live: ScanProgress | null } | { phase: "result"; report: ScanReport };

type Confirm =
  | { kind: "revokeTool"; skill: SkillMeta; toolId: string }
  | { kind: "revokeAll"; skill: SkillMeta; toolIds: string[] }
  | { kind: "delete"; skill: SkillMeta; toolIds: string[] }
  | null;

const ORIGIN_LABEL: Record<SkillMeta["originKind"], string> = {
  local: "本地",
  git: "Git",
  archive: "压缩包",
};

function toolBadgeLabel(t: ToolDto): string {
  return (t.icon || t.displayName).slice(0, 2);
}

export default function LibraryPage() {
  const library = useLibrary();
  const targets = useTargets();
  const tools = useTools();
  const deployApply = useDeployApply();
  const deployRevoke = useDeployRevoke();
  const removeSkill = useRemoveSkill();
  const install = useInstallLocal();
  const toast = useToast();
  const qc = useQueryClient();

  const [search, setSearch] = useState("");
  const [origin, setOrigin] = useState<OriginFilter>("all");
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [rescan, setRescan] = useState<RescanState>({ phase: "idle" });
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [showTags, setShowTags] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  // 详情导航：按 id 派生，库数据刷新（更新/删除）后自动跟随或回落列表。
  const selected = useMemo(
    () => (selectedId == null ? null : (library.data ?? []).find((s) => s.id === selectedId) ?? null),
    [selectedId, library.data],
  );

  // §4.3 只读来源：正本位于任一工具插件缓存内的 Skill（后端另有部署硬守卫）。
  const readOnlyById = useMemo(() => {
    const m = new Map<number, boolean>();
    for (const s of library.data ?? []) {
      m.set(s.id, isReadOnlySkill(s.canonicalPath, tools.data ?? []));
    }
    return m;
  }, [library.data, tools.data]);

  // 部署意图索引：skillId → 已部署工具集合。
  const deployedBySkill = useMemo(() => {
    const m = new Map<number, Set<string>>();
    for (const t of targets.data ?? []) {
      const set = m.get(t.skillId) ?? new Set<string>();
      set.add(t.toolId);
      m.set(t.skillId, set);
    }
    return m;
  }, [targets.data]);

  // 可部署工具：已启用 + 已安装。
  const deployableTools = useMemo(
    () => (tools.data ?? []).filter((t) => t.enabled && t.installed),
    [tools.data],
  );
  const toolById = useMemo(() => {
    const m = new Map<string, ToolDto>();
    for (const t of tools.data ?? []) {
      m.set(t.id, t);
    }
    return m;
  }, [tools.data]);

  // 过滤（搜索 + 来源；标签过滤见 showTags 弹层）。
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return (library.data ?? []).filter((s) => {
      if (origin !== "all" && s.originKind !== origin) {
        return false;
      }
      if (q) {
        const hay = `${s.name}\n${s.description}\n${s.originRef}`.toLowerCase();
        if (!hay.includes(q)) {
          return false;
        }
      }
      return true;
    });
  }, [library.data, origin, search]);

  // ------------------------------------------------------------------
  // 重新扫描（信息性：更新磁盘真相；未入库目录可逐个原地登记）。
  // ------------------------------------------------------------------
  useEffect(() => {
    if (rescan.phase !== "running") {
      return;
    }
    let cancelled = false;
    let off: UnlistenFn | null = null;
    let timer = 0;
    const finish = (report: ScanReport) => {
      window.clearInterval(timer);
      if (!cancelled) {
        setRescan({ phase: "result", report });
      }
    };
    void (async () => {
      off = await listenScanEvents({
        onProgress: (p) => setRescan({ phase: "running", live: p }),
        onDone: finish,
        onError: (m) => {
          window.clearInterval(timer);
          if (!cancelled) {
            setRescan({ phase: "idle" });
            toast.show(`扫描失败：${m}`, "danger");
          }
        },
      });
      const st = await scanStatus();
      if (cancelled) {
        return;
      }
      if (!st.running && !st.report) {
        try {
          await scanStart();
        } catch (e) {
          setRescan({ phase: "idle" });
          toast.show(ipcError(e), "danger");
        }
      } else if (!st.running && st.report) {
        finish(st.report);
      }
      timer = window.setInterval(() => {
        void scanStatus()
          .then((s) => {
            if (cancelled || s.running) {
              return;
            }
            if (s.report) {
              finish(s.report);
            }
          })
          .catch(() => {});
      }, 1200);
    })();
    return () => {
      cancelled = true;
      off?.();
      window.clearInterval(timer);
    };
  }, [rescan.phase, toast]);

  // 重新扫描结果：哪些发现目录尚未入库。
  const unregistered = useMemo(() => {
    if (rescan.phase !== "result") {
      return [];
    }
    const known = new Set<string>();
    for (const s of library.data ?? []) {
      known.add(s.canonicalPath);
    }
    for (const t of targets.data ?? []) {
      known.add(t.path);
    }
    return rescan.report.skills.filter((s) => !known.has(s.path));
  }, [rescan, library.data, targets.data]);

  // ------------------------------------------------------------------
  // 操作
  // ------------------------------------------------------------------
  function applyOne(skill: SkillMeta, toolId: string) {
    deployApply.mutate(
      { skillId: skill.id, tools: [toolId], mode: "symlink", overwrite: false, dryRun: false },
      {
        onSuccess: () => toast.show(`已部署到 ${toolById.get(toolId)?.displayName ?? toolId}`, "success"),
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  function runConfirm() {
    if (!confirm) {
      return;
    }
    if (confirm.kind === "revokeTool") {
      deployRevoke.mutate(
        { skillId: confirm.skill.id, tools: [confirm.toolId], dryRun: false },
        {
          onSuccess: () => toast.show(`已取消部署到 ${toolById.get(confirm.toolId)?.displayName ?? confirm.toolId}`, "success"),
          onError: (e) => toast.show(ipcError(e), "danger"),
        },
      );
    } else if (confirm.kind === "revokeAll") {
      deployRevoke.mutate(
        { skillId: confirm.skill.id, tools: [], dryRun: false },
        {
          onSuccess: () => toast.show(`已取消「${confirm.skill.name}」的全部部署`, "success"),
          onError: (e) => toast.show(ipcError(e), "danger"),
        },
      );
    } else {
      removeSkill.mutate(
        { skillId: confirm.skill.id, mode: "delete", dryRun: false },
        {
          onSuccess: () => toast.show(`「${confirm.skill.name}」正本已移入回收站`, "success"),
          onError: (e) => toast.show(ipcError(e), "danger"),
        },
      );
    }
    setConfirm(null);
  }

  function registerDiscovered(path: string, name: string | null) {
    install.mutate(
      { path, name, keepInPlace: true, dryRun: false },
      {
        onSuccess: () => {
          toast.show("已原地登记", "success");
          void qc.invalidateQueries({ queryKey: queryKeys.library });
          void qc.invalidateQueries({ queryKey: queryKeys.targets });
        },
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  // ------------------------------------------------------------------
  // 卡片 / 行渲染
  // ------------------------------------------------------------------
  function agentBadges(skill: SkillMeta) {
    const deployed = deployedBySkill.get(skill.id) ?? new Set<string>();
    const readOnly = readOnlyById.get(skill.id) ?? false;
    return deployableTools.map((t) => {
      const isDeployed = deployed.has(t.id);
      return (
        <button
          key={t.id}
          type="button"
          disabled={readOnly && !isDeployed}
          title={
            readOnly
              ? "只读来源（插件缓存），不可部署"
              : isDeployed
                ? `已部署到 ${t.displayName}，点击取消部署`
                : `点击部署到 ${t.displayName}`
          }
          className={[
            "inline-flex h-6 items-center rounded-tag px-2 text-xs font-semibold transition-colors duration-120",
            isDeployed
              ? "bg-primary text-white hover:bg-primary-hover"
              : "border border-line bg-surface text-ink-faint hover:border-primary hover:text-primary-active",
          ].join(" ")}
          onClick={(e) => {
            e.stopPropagation();
            if (isDeployed) {
              setConfirm({ kind: "revokeTool", skill, toolId: t.id });
            } else if (!readOnly) {
              applyOne(skill, t.id);
            }
          }}
        >
          {toolBadgeLabel(t)}
        </button>
      );
    });
  }

  function skillActions(skill: SkillMeta) {
    const deployedTools = [...(deployedBySkill.get(skill.id) ?? new Set<string>())];
    return (
      <>
        <Button
          variant="ghost"
          size="sm"
          disabled={deployedTools.length === 0 || deployRevoke.isPending}
          onClick={() => setConfirm({ kind: "revokeAll", skill, toolIds: deployedTools })}
        >
          <Unlink size={13} />
          取消部署{deployedTools.length > 0 && `（${deployedTools.length}）`}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="text-danger hover:text-danger"
          disabled={removeSkill.isPending}
          onClick={() => setConfirm({ kind: "delete", skill, toolIds: deployedTools })}
        >
          <Trash2 size={13} />
          删除 Skill
        </Button>
      </>
    );
  }

  function renderCard(skill: SkillMeta) {
    return (
      <div
        key={skill.id}
        className="sb-card sb-card--interactive flex cursor-pointer flex-col p-4"
        onClick={() => setSelectedId(skill.id)}
      >
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 truncate text-sm font-medium text-ink" title={skill.name}>
            {skill.name}
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <Badge tone={skill.originKind === "local" ? "neutral" : "blue"}>{ORIGIN_LABEL[skill.originKind]}</Badge>
            {(readOnlyById.get(skill.id) ?? false) && <Badge tone="danger">只读</Badge>}
            <Badge tone="neutral">未体检</Badge>
          </div>
        </div>
        <p className="mt-1.5 line-clamp-2 min-h-8 text-xs leading-4 text-ink-secondary">
          {skill.description || "（无描述）"}
        </p>
        <div className="mt-2.5 flex items-center gap-1.5">{agentBadges(skill)}</div>
        <div className="mt-3 flex items-center gap-1 border-t border-line pt-2.5">
          {skillActions(skill)}
        </div>
      </div>
    );
  }

  function renderRow(skill: SkillMeta) {
    return (
      <div
        key={skill.id}
        className="flex h-[60px] cursor-pointer items-center gap-3 border-b border-line px-4 transition-colors duration-120 hover:bg-surface"
        onClick={() => setSelectedId(skill.id)}
      >
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm text-ink">{skill.name}</span>
            <Badge tone={skill.originKind === "local" ? "neutral" : "blue"}>{ORIGIN_LABEL[skill.originKind]}</Badge>
            {(readOnlyById.get(skill.id) ?? false) && <Badge tone="danger">只读</Badge>}
          </div>
          <div className="truncate text-xs text-ink-faint">{skill.description || skill.canonicalPath}</div>
        </div>
        <div className="flex items-center gap-1.5">{agentBadges(skill)}</div>
        <div className="flex items-center gap-1">{skillActions(skill)}</div>
      </div>
    );
  }

  // ------------------------------------------------------------------
  // 页面
  // ------------------------------------------------------------------
  // 详情页接管整个技能库区域（§9.4）；skill 被删除后自动回落列表。
  if (selected) {
    return <SkillDetailPage skill={selected} onBack={() => setSelectedId(null)} />;
  }

  const error = library.isError ? library.error : targets.isError ? targets.error : tools.isError ? tools.error : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-8 py-6">
      {/* 顶栏：搜索 · 来源过滤 · 标签 · 视图切换 · 扫描（§11.3） */}
      <div className="flex items-center gap-2">
        <div className="relative">
          <Search size={14} className="absolute top-1/2 left-2.5 -translate-y-1/2 text-ink-faint" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索名称 / 描述 / 来源"
            className="h-8 w-56 rounded-btn border border-line bg-page pl-8 pr-3 text-sm text-ink placeholder:text-ink-faint focus:border-primary focus:outline-none"
          />
        </div>
        <div className="flex items-center gap-0.5 rounded-btn border border-line bg-page p-0.5">
          {(["all", "local", "git", "archive"] as OriginFilter[]).map((f) => (
            <button
              key={f}
              type="button"
              className={`h-7 rounded-[6px] px-2.5 text-xs transition-colors duration-120 ${
                origin === f ? "bg-primary-soft font-medium text-primary-active" : "text-ink-secondary hover:bg-surface"
              }`}
              onClick={() => setOrigin(f)}
            >
              {f === "all" ? "全部" : ORIGIN_LABEL[f]}
            </button>
          ))}
        </div>
        <Button variant="ghost" size="sm" onClick={() => setShowTags(true)}>
          标签
        </Button>
        <div className="flex-1" />
        <div className="flex items-center gap-0.5 rounded-btn border border-line bg-page p-0.5">
          {(
            [
              ["grid", <LayoutGrid key="g" size={14} />],
              ["list", <List key="l" size={14} />],
            ] as [ViewMode, React.ReactNode][]
          ).map(([mode, icon]) => (
            <button
              key={mode}
              type="button"
              aria-label={mode === "grid" ? "网格视图" : "列表视图"}
              className={`flex h-7 w-8 items-center justify-center rounded-[6px] transition-colors duration-120 ${
                viewMode === mode ? "bg-primary-soft text-primary-active" : "text-ink-faint hover:bg-surface"
              }`}
              onClick={() => setViewMode(mode)}
            >
              {icon}
            </button>
          ))}
        </div>
        <Button
          variant="primary"
          size="sm"
          disabled={rescan.phase === "running"}
          onClick={() => setRescan({ phase: "running", live: null })}
        >
          <RefreshCw size={13} className={rescan.phase === "running" ? "animate-spin" : ""} />
          {rescan.phase === "running" ? "扫描中…" : "扫描"}
        </Button>
      </div>

      {/* 内容 */}
      <div className="mt-4 min-h-0 flex-1">
        {error ? (
          <EmptyState
            title="数据加载失败"
            description={ipcError(error)}
            action={
              <Button variant="primary" size="sm" onClick={() => void qc.invalidateQueries()}>
                重试
              </Button>
            }
          />
        ) : library.isLoading || targets.isLoading || tools.isLoading ? (
          viewMode === "grid" ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
              {[0, 1, 2, 3, 4, 5].map((i) => (
                <Skeleton key={i} className="h-40 rounded-card" />
              ))}
            </div>
          ) : (
            <div className="space-y-1.5">
              {[0, 1, 2, 3, 4, 5, 6, 7].map((i) => (
                <Skeleton key={i} className="h-[60px] w-full rounded-[8px]" />
              ))}
            </div>
          )
        ) : (library.data ?? []).length === 0 ? (
          <EmptyState
            title="技能库还是空的"
            description="运行一次扫描，把各 AI 工具目录里已有的 Skill 登记进来。"
            action={
              <Button variant="primary" size="sm" onClick={() => setRescan({ phase: "running", live: null })}>
                开始扫描
              </Button>
            }
          />
        ) : filtered.length === 0 ? (
          <EmptyState title="没有匹配的 Skill" description="试试更换关键词或来源过滤条件。" />
        ) : viewMode === "grid" ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
            {filtered.map(renderCard)}
          </div>
        ) : filtered.length > 300 ? (
          <div className="overflow-hidden rounded-card border border-line bg-page">
            {/* >300 条：自建虚拟滚动（§11 DoD） */}
            <VirtualList
              items={filtered}
              itemHeight={60}
              height={560}
              renderItem={renderRow}
            />
          </div>
        ) : (
          <div className="overflow-hidden rounded-card border border-line bg-page">{filtered.map(renderRow)}</div>
        )}
      </div>

      {/* 标签过滤（v1：skill_tags 暂无写入来源，诚实空态） */}
      <Modal open={showTags} onClose={() => setShowTags(false)} title="标签过滤">
        <EmptyState title="暂无标签" description="标签编辑将在后续版本提供；当前可用来源过滤缩小范围。" />
      </Modal>

      {/* 重新扫描结果 */}
      <Modal
        open={rescan.phase === "result"}
        onClose={() => setRescan({ phase: "idle" })}
        title="扫描完成"
        footer={
          <Button variant="primary" onClick={() => setRescan({ phase: "idle" })}>
            关闭
          </Button>
        }
      >
        {rescan.phase === "result" && (
          <>
            <p>
              发现 {rescan.report.skills.length} 个 Skill 目录，其中{" "}
              {rescan.report.skills.length - unregistered.length} 个已入库。
              {rescan.report.skills.length - unregistered.length > 0 && " 已入库的目录未做任何改动。"}
            </p>
            {unregistered.length > 0 && (
              <div className="mt-3 space-y-1.5">
                <p className="text-xs text-ink-faint">以下目录尚未登记（登记不会移动文件）：</p>
                <div className="max-h-44 space-y-1 overflow-y-auto">
                  {unregistered.map((s) => (
                    <div key={s.path} className="flex items-center gap-2 rounded-[8px] bg-surface px-2.5 py-1.5">
                      <span className="min-w-0 flex-1 truncate text-xs text-ink">{s.name ?? s.dirName}</span>
                      <Button size="sm" onClick={() => registerDiscovered(s.path, s.name)}>
                        登记
                      </Button>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </Modal>

      {/* 重新扫描运行中提示 */}
      <Modal open={rescan.phase === "running"} onClose={() => setRescan({ phase: "idle" })} title="正在扫描…">
        {rescan.phase === "running" && (
          <>
            <div className="sb-pulse h-1.5 w-full overflow-hidden rounded-full bg-line">
              <div className="h-full w-1/3 rounded-full bg-primary" />
            </div>
            <p className="mt-3">
              {rescan.live
                ? `已扫描 ${rescan.live.scanned} 个目录 · 已发现 ${rescan.live.found} 个 Skill`
                : "正在启动扫描…"}
            </p>
          </>
        )}
      </Modal>

      {/* 两级删除语义：一级「取消部署」 */}
      <Modal
        open={confirm?.kind === "revokeTool" || confirm?.kind === "revokeAll"}
        onClose={() => setConfirm(null)}
        title="取消部署"
        footer={
          <>
            <Button onClick={() => setConfirm(null)}>再想想</Button>
            <Button variant="primary" onClick={runConfirm}>
              确认取消部署
            </Button>
          </>
        }
      >
        {confirm?.kind === "revokeTool" && (
          <p>
            将移除「{confirm.skill.name}」在{" "}
            {toolById.get(confirm.toolId)?.displayName ?? confirm.toolId} 的部署（链接 / 副本移入回收站）。
            <span className="font-medium text-ink">正本仍在中央库，随时可重新部署。</span>
          </p>
        )}
        {confirm?.kind === "revokeAll" && (
          <p>
            将移除「{confirm.skill.name}」在 {confirm.toolIds.length} 个工具的部署（链接 / 副本移入回收站）。
            <span className="font-medium text-ink">正本仍在中央库，随时可重新部署。</span>
          </p>
        )}
      </Modal>

      {/* 两级删除语义：二级「删除 Skill（正本入回收站）」 */}
      <Modal
        open={confirm?.kind === "delete"}
        onClose={() => setConfirm(null)}
        title="删除 Skill"
        footer={
          <>
            <Button onClick={() => setConfirm(null)}>取消</Button>
            <Button variant="danger" onClick={runConfirm}>
              确认删除
            </Button>
          </>
        }
      >
        {confirm?.kind === "delete" && (
          <p>
            即将删除「{confirm.skill.name}」：
            {confirm.toolIds.length > 0 ? `撤销它在 ${confirm.toolIds.length} 个工具的部署；` : ""}
            正本将移入回收站，可在后续版本的「工具箱」中恢复。
            <span className="font-medium text-ink">此操作不会直接抹除文件。</span>
          </p>
        )}
      </Modal>
    </div>
  );
}
