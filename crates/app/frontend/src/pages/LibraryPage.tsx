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
  useSecurityReport,
  useTargets,
  useTools,
} from "../api/queries";
import VirtualList from "../components/VirtualList";
import { RiskBadge } from "../components/SecurityKit";
import { ScanResultModal, ScanRunningModal, TagFilterModal } from "../components/ScanModals";
import { useToast } from "../components/Toast";
import { Badge, Button, EmptyState, Modal, Skeleton } from "../components/ui";
import { useI18n } from "../i18n";
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

const ORIGIN_LABEL_KEY: Record<SkillMeta["originKind"], string> = {
  local: "common.origin.local",
  git: "common.origin.git",
  archive: "common.origin.archive",
};

function toolBadgeLabel(x: ToolDto): string {
  return (x.icon || x.displayName).slice(0, 2);
}

/// 卡片 / 行内的风险徽章：挂载即触发纯静态扫描（后端缓存，幂等廉价）。
function CardRiskBadge({ skillId }: { skillId: number }) {
  const security = useSecurityReport(skillId);
  return <RiskBadge level={security.data?.riskLevel} scanned={!!security.data} />;
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
  const { t } = useI18n();

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
    for (const x of targets.data ?? []) {
      const set = m.get(x.skillId) ?? new Set<string>();
      set.add(x.toolId);
      m.set(x.skillId, set);
    }
    return m;
  }, [targets.data]);

  // 可部署工具：已启用 + 已安装。
  const deployableTools = useMemo(
    () => (tools.data ?? []).filter((x) => x.enabled && x.installed),
    [tools.data],
  );
  const toolById = useMemo(() => {
    const m = new Map<string, ToolDto>();
    for (const x of tools.data ?? []) {
      m.set(x.id, x);
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
            toast.show(t("common.scanFailed", { msg: m }), "danger");
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
  }, [rescan.phase, toast, t]);

  // 重新扫描结果：哪些发现目录尚未入库。
  const unregistered = useMemo(() => {
    if (rescan.phase !== "result") {
      return [];
    }
    const known = new Set<string>();
    for (const s of library.data ?? []) {
      known.add(s.canonicalPath);
    }
    for (const x of targets.data ?? []) {
      known.add(x.path);
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
        onSuccess: () => toast.show(t("common.deployToast", { tool: toolById.get(toolId)?.displayName ?? toolId }), "success"),
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
          onSuccess: () =>
            toast.show(
              t("common.revokeToast", { tool: toolById.get(confirm.toolId)?.displayName ?? confirm.toolId }),
              "success",
            ),
          onError: (e) => toast.show(ipcError(e), "danger"),
        },
      );
    } else if (confirm.kind === "revokeAll") {
      deployRevoke.mutate(
        { skillId: confirm.skill.id, tools: [], dryRun: false },
        {
          onSuccess: () => toast.show(t("library.revokeAllToast", { skill: confirm.skill.name }), "success"),
          onError: (e) => toast.show(ipcError(e), "danger"),
        },
      );
    } else {
      removeSkill.mutate(
        { skillId: confirm.skill.id, mode: "delete", dryRun: false },
        {
          onSuccess: () => toast.show(t("library.deleteToast", { skill: confirm.skill.name }), "success"),
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
          toast.show(t("library.registeredToast"), "success");
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
    return deployableTools.map((x) => {
      const isDeployed = deployed.has(x.id);
      return (
        <button
          key={x.id}
          type="button"
          disabled={readOnly && !isDeployed}
          title={
            readOnly
              ? t("library.badgeReadOnlyTitle")
              : isDeployed
                ? t("library.badgeDeployedTitle", { tool: x.displayName })
                : t("library.badgeDeployTitle", { tool: x.displayName })
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
              setConfirm({ kind: "revokeTool", skill, toolId: x.id });
            } else if (!readOnly) {
              applyOne(skill, x.id);
            }
          }}
        >
          {toolBadgeLabel(x)}
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
          {deployedTools.length > 0 ? t("library.undeployCount", { n: deployedTools.length }) : t("library.undeploy")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="text-danger hover:text-danger"
          disabled={removeSkill.isPending}
          onClick={() => setConfirm({ kind: "delete", skill, toolIds: deployedTools })}
        >
          <Trash2 size={13} />
          {t("library.deleteSkill")}
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
            <Badge tone={skill.originKind === "local" ? "neutral" : "blue"}>{t(ORIGIN_LABEL_KEY[skill.originKind])}</Badge>
            {(readOnlyById.get(skill.id) ?? false) && <Badge tone="danger">{t("common.readOnly")}</Badge>}
            <CardRiskBadge skillId={skill.id} />
          </div>
        </div>
        <p className="mt-1.5 line-clamp-2 min-h-8 text-xs leading-4 text-ink-secondary">
          {skill.description || t("common.noDescription")}
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
            <Badge tone={skill.originKind === "local" ? "neutral" : "blue"}>{t(ORIGIN_LABEL_KEY[skill.originKind])}</Badge>
            {(readOnlyById.get(skill.id) ?? false) && <Badge tone="danger">{t("common.readOnly")}</Badge>}
            <CardRiskBadge skillId={skill.id} />
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
            placeholder={t("library.searchPlaceholder")}
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
              {f === "all" ? t("library.filterAll") : t(ORIGIN_LABEL_KEY[f])}
            </button>
          ))}
        </div>
        <Button variant="ghost" size="sm" onClick={() => setShowTags(true)}>
          {t("library.tagFilterTitle")}
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
              aria-label={mode === "grid" ? t("library.gridView") : t("library.listView")}
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
          {rescan.phase === "running" ? t("library.scanning") : t("library.scan")}
        </Button>
      </div>

      {/* 内容 */}
      <div className="mt-4 min-h-0 flex-1">
        {error ? (
          <EmptyState
            title={t("library.errorTitle")}
            description={ipcError(error)}
            action={
              <Button variant="primary" size="sm" onClick={() => void qc.invalidateQueries()}>
                {t("common.retry")}
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
            title={t("library.emptyTitle")}
            description={t("library.emptyDesc")}
            action={
              <Button variant="primary" size="sm" onClick={() => setRescan({ phase: "running", live: null })}>
                {t("library.startScan")}
              </Button>
            }
          />
        ) : filtered.length === 0 ? (
          <EmptyState title={t("library.noMatchTitle")} description={t("library.noMatchDesc")} />
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
      <TagFilterModal open={showTags} onClose={() => setShowTags(false)} />

      {/* 重新扫描结果 */}
      {rescan.phase === "result" && (
        <ScanResultModal
          report={rescan.report}
          unregistered={unregistered}
          onClose={() => setRescan({ phase: "idle" })}
          onRegister={registerDiscovered}
        />
      )}

      {/* 重新扫描运行中提示 */}
      {rescan.phase === "running" && (
        <ScanRunningModal live={rescan.live} onClose={() => setRescan({ phase: "idle" })} />
      )}

      {/* 两级删除语义：一级「取消部署」 */}
      <Modal
        open={confirm?.kind === "revokeTool" || confirm?.kind === "revokeAll"}
        onClose={() => setConfirm(null)}
        title={t("library.revokeTitle")}
        footer={
          <>
            <Button onClick={() => setConfirm(null)}>{t("common.holdOn")}</Button>
            <Button variant="primary" onClick={runConfirm}>
              {t("library.revokeOk")}
            </Button>
          </>
        }
      >
        {confirm?.kind === "revokeTool" && (
          <p>
            {t("library.revokeOne", {
              skill: confirm.skill.name,
              tool: toolById.get(confirm.toolId)?.displayName ?? confirm.toolId,
            })}
            <span className="font-medium text-ink">{t("library.revokeNote")}</span>
          </p>
        )}
        {confirm?.kind === "revokeAll" && (
          <p>
            {t("library.revokeAll", { skill: confirm.skill.name, n: confirm.toolIds.length })}
            <span className="font-medium text-ink">{t("library.revokeNote")}</span>
          </p>
        )}
      </Modal>

      {/* 两级删除语义：二级「删除 Skill（正本入回收站）」 */}
      <Modal
        open={confirm?.kind === "delete"}
        onClose={() => setConfirm(null)}
        title={t("library.deleteTitle")}
        footer={
          <>
            <Button onClick={() => setConfirm(null)}>{t("common.cancel")}</Button>
            <Button variant="danger" onClick={runConfirm}>
              {t("library.deleteOk")}
            </Button>
          </>
        }
      >
        {confirm?.kind === "delete" && (
          <p>
            {t("library.deleteBodyPrefix", { skill: confirm.skill.name })}
            {confirm.toolIds.length > 0 && t("library.deleteBodyRevoke", { n: confirm.toolIds.length })}
            {t("library.deleteBodyTrash")}
            <span className="font-medium text-ink">{t("library.deleteBodyNote")}</span>
          </p>
        )}
      </Modal>
    </div>
  );
}
