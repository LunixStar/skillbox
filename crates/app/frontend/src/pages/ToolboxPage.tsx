/// 工具箱页（§11.3 / §9）：安全扫描总览 + 上下文成本分析。
/// 专业功能收进折叠区，不污染主界面（库 / 工具 / 体检）。

import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { RefreshCw, ShieldCheck, Unlink } from "lucide-react";
import {
  ipcError,
  securityScan,
  type ConflictGroup,
  type SkillMeta,
  type ToolDto,
} from "../api/ipc";
import {
  queryKeys,
  useCostReport,
  useDeployRevoke,
  useLibrary,
  useSecurityOverview,
  useSecurityReport,
  useTargets,
  useTools,
} from "../api/queries";
import { useToast } from "../components/Toast";
import { FindingList, RiskBadge } from "../components/SecurityKit";
import { Badge, Button, EmptyState, Skeleton, Tag } from "../components/ui";

// ---------------------------------------------------------------------------
// 安全扫描：逐 skill 体检（后端缓存聚合；纯静态，只提示不阻断）
// ---------------------------------------------------------------------------

function SkillRiskRow({ skill }: { skill: SkillMeta }) {
  const report = useSecurityReport(skill.id);
  const [open, setOpen] = useState(false);
  const findings = report.data?.findings ?? [];
  return (
    <div className="rounded-[8px] border border-line bg-page">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-ink">{skill.name}</span>
        {report.isLoading ? (
          <Skeleton className="h-5 w-12 rounded-[6px]" />
        ) : (
          <>
            <RiskBadge level={report.data?.riskLevel} scanned={!!report.data} />
            <span className="w-14 shrink-0 text-right text-xs text-ink-faint">
              {findings.length > 0 ? `${findings.length} 项` : "—"}
            </span>
          </>
        )}
      </button>
      {open && report.data && (
        <div className="border-t border-line px-2.5 py-2">
          <FindingList findings={findings} />
        </div>
      )}
    </div>
  );
}

function SecuritySection() {
  const library = useLibrary();
  const overview = useSecurityOverview();
  const toast = useToast();
  const qc = useQueryClient();
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState({ done: 0, total: 0 });

  async function rescanAll() {
    const skills = library.data ?? [];
    if (skills.length === 0 || scanning) {
      return;
    }
    setScanning(true);
    setProgress({ done: 0, total: skills.length });
    try {
      for (const skill of skills) {
        await securityScan(skill.id);
        setProgress((p) => ({ ...p, done: p.done + 1 }));
      }
      await qc.invalidateQueries({ queryKey: queryKeys.securityOverview });
      await qc.invalidateQueries({ queryKey: ["security"] });
      toast.show(`已完成 ${skills.length} 个 Skill 的安全体检`, "success");
    } catch (e) {
      toast.show(ipcError(e), "danger");
    } finally {
      setScanning(false);
    }
  }

  const total = library.data?.length ?? 0;
  const overviewFindings = overview.data?.findings.length ?? 0;

  return (
    <section className="sb-card p-4">
      <div className="flex items-center gap-2">
        <ShieldCheck size={15} className="text-primary" />
        <h3 className="text-sm font-semibold text-ink">安全扫描</h3>
        <Badge tone="neutral">静态规则 · 只提示不阻断</Badge>
        <div className="flex-1" />
        <Button variant="secondary" size="sm" disabled={scanning || total === 0} onClick={() => void rescanAll()}>
          <RefreshCw size={13} className={scanning ? "animate-spin" : ""} />
          {scanning ? `体检中 ${progress.done}/${progress.total}` : "全部体检"}
        </Button>
      </div>
      {overview.isLoading ? (
        <Skeleton className="mt-3 h-8 w-full rounded-[8px]" />
      ) : (
        <div className="mt-3 flex items-center gap-2 border-t border-line pt-3">
          <RiskBadge level={overview.data?.riskLevel} scanned={(overview.data?.findings.length ?? 0) > 0} />
          <p className="text-xs leading-5 text-ink-secondary">
            {overviewFindings > 0
              ? `共 ${overviewFindings} 条发现。逐个 Skill 的明细见下方列表。`
              : "还没有已汇总的发现。点击「全部体检」对技能库做一轮纯静态扫描。"}
          </p>
        </div>
      )}
      <div className="mt-3 space-y-1.5">
        {library.isLoading ? (
          [0, 1, 2].map((i) => <Skeleton key={i} className="h-10 w-full rounded-[8px]" />)
        ) : total === 0 ? (
          <p className="text-xs text-ink-faint">技能库为空。</p>
        ) : (
          (library.data ?? []).map((s) => <SkillRiskRow key={s.id} skill={s} />)
        )}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// 上下文成本（§9.2）：估算 token + 冲突组；纯 CSS 条形图，无图表库
// ---------------------------------------------------------------------------

function ConflictCard({
  group,
  toolById,
}: {
  group: ConflictGroup;
  toolById: Map<string, ToolDto>;
}) {
  const revoke = useDeployRevoke();
  const toast = useToast();
  const toolName = toolById.get(group.members[0]?.toolId ?? "")?.displayName ?? group.members[0]?.toolId ?? "";

  function deactivate(skillId: number, toolId: string, name: string) {
    revoke.mutate(
      { skillId, tools: [toolId], dryRun: false },
      {
        onSuccess: () => toast.show(`已停用「${name}」在 ${toolName} 的常驻`, "success"),
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  return (
    <div className="rounded-[8px] border border-line bg-page p-2.5">
      <div className="flex items-center gap-1.5">
        <Badge tone="blue">触发词冲突</Badge>
        <span className="text-xs text-ink-secondary">
          {toolName} · 相似度 {(group.similarity * 100).toFixed(0)}%
        </span>
      </div>
      {group.keywords.length > 0 && (
        <div className="mt-1.5 flex flex-wrap gap-1">
          {group.keywords.map((k) => (
            <Tag key={k}>{k}</Tag>
          ))}
        </div>
      )}
      <div className="mt-2 space-y-1">
        {group.members.map((m) => (
          <div key={`${m.skillId}-${m.toolId}`} className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-xs text-ink">{m.name}</span>
            <Button
              variant="ghost"
              size="sm"
              disabled={revoke.isPending}
              onClick={() => deactivate(m.skillId, m.toolId, m.name)}
            >
              <Unlink size={12} />
              停用
            </Button>
          </div>
        ))}
      </div>
      <p className="mt-1.5 text-xs leading-4 text-ink-faint">
        两个 description 高度相似时，触发词会互相争抢；停用其一可降低误触发。
      </p>
    </div>
  );
}

function CostSection() {
  const cost = useCostReport();
  const tools = useTools();
  const targets = useTargets();

  const toolById = new Map<string, ToolDto>((tools.data ?? []).map((t) => [t.id, t]));
  const perTool = cost.data?.perTool ?? [];
  const conflicts = cost.data?.conflicts ?? [];
  const totalSkills = perTool.reduce((s, t) => s + t.skillCount, 0);
  const totalTokens = perTool.reduce((s, t) => s + t.estimatedTokens, 0);
  const maxTokens = Math.max(1, ...perTool.map((t) => t.estimatedTokens));

  return (
    <section className="sb-card p-4">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-semibold text-ink">上下文成本</h3>
        <Badge tone="neutral">估算值 · ~3.5 字符/token</Badge>
      </div>
      {cost.isLoading ? (
        <Skeleton className="mt-3 h-24 w-full rounded-[8px]" />
      ) : (
        <>
          {/* 数字卡片 */}
          <div className="mt-3 grid grid-cols-2 gap-2 border-t border-line pt-3">
            <div className="rounded-[8px] bg-surface px-3 py-2.5">
              <div className="text-lg font-semibold text-ink">{totalSkills}</div>
              <div className="text-xs text-ink-faint">常驻 skill 总数（已启用工具）</div>
            </div>
            <div className="rounded-[8px] bg-surface px-3 py-2.5">
              <div className="text-lg font-semibold text-ink">
                {totalTokens.toLocaleString()}
                <span className="ml-1 text-xs font-normal text-ink-faint">tokens（估）</span>
              </div>
              <div className="text-xs text-ink-faint">description 常驻成本估算</div>
            </div>
          </div>
          {/* 按工具分组的条形图（纯 CSS，无图表库） */}
          <div className="mt-3 space-y-2 border-t border-line pt-3">
            {perTool.length === 0 ? (
              <p className="text-xs text-ink-faint">没有已启用的工具。</p>
            ) : (
              perTool.map((t) => (
                <div key={t.toolId} className="flex items-center gap-2">
                  <span className="w-28 shrink-0 truncate text-xs text-ink-secondary" title={toolById.get(t.toolId)?.displayName ?? t.toolId}>
                    {toolById.get(t.toolId)?.displayName ?? t.toolId}
                  </span>
                  <div className="h-4 min-w-0 flex-1 overflow-hidden rounded-[4px] bg-line">
                    <div
                      className="h-full rounded-[4px] bg-primary transition-transform duration-200 ease-skillbox"
                      style={{ width: `${(t.estimatedTokens / maxTokens) * 100}%` }}
                    />
                  </div>
                  <span className="w-32 shrink-0 text-right text-xs text-ink-faint">
                    {t.skillCount} 个 · {t.estimatedTokens.toLocaleString()} tokens（估）
                  </span>
                </div>
              ))
            )}
          </div>
          {/* 冲突组 */}
          <div className="mt-3 border-t border-line pt-3">
            <h4 className="text-xs font-semibold text-ink">触发词冲突风险组（Jaccard ≥ 0.35）</h4>
            <div className="mt-2 space-y-2">
              {conflicts.length === 0 ? (
                <p className="text-xs text-ink-faint">未发现冲突风险的 description 组。</p>
              ) : (
                conflicts.map((g, i) => <ConflictCard key={i} group={g} toolById={toolById} />)
              )}
            </div>
          </div>
        </>
      )}
      {targets.isError && (
        <p className="mt-2 text-xs text-danger">部署数据加载失败：{ipcError(targets.error)}</p>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// 页面
// ---------------------------------------------------------------------------

export default function ToolboxPage() {
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-8 py-6">
      <div className="flex items-center gap-2">
        <h2 className="text-base font-semibold text-ink">工具箱</h2>
        <p className="text-xs text-ink-faint">专业功能折叠区：安全扫描与上下文成本分析。</p>
      </div>
      <div className="mt-4 grid max-w-5xl grid-cols-1 items-start gap-3 xl:grid-cols-2">
        <SecuritySection />
        <CostSection />
      </div>
      <div className="mt-3 max-w-5xl">
        <EmptyState
          title="更多工具整理中"
          description="日志导出、缓存与回收站管理将在后续版本加入工具箱。"
        />
      </div>
    </div>
  );
}
