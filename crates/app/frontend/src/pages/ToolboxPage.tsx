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
import { useI18n } from "../i18n";

// ---------------------------------------------------------------------------
// 安全扫描：逐 skill 体检（后端缓存聚合；纯静态，只提示不阻断）
// ---------------------------------------------------------------------------

function SkillRiskRow({ skill }: { skill: SkillMeta }) {
  const report = useSecurityReport(skill.id);
  const { t } = useI18n();
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
              {findings.length > 0 ? t("toolbox.findingsCount", { n: findings.length }) : "—"}
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
  const { t } = useI18n();
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
      toast.show(t("toolbox.securityDoneToast", { n: skills.length }), "success");
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
        <h3 className="text-sm font-semibold text-ink">{t("toolbox.securityTitle")}</h3>
        <Badge tone="neutral">{t("toolbox.securityBadge")}</Badge>
        <div className="flex-1" />
        <Button variant="secondary" size="sm" disabled={scanning || total === 0} onClick={() => void rescanAll()}>
          <RefreshCw size={13} className={scanning ? "animate-spin" : ""} />
          {scanning
            ? t("toolbox.securityScanning", { done: progress.done, total: progress.total })
            : t("toolbox.securityRescanAll")}
        </Button>
      </div>
      {overview.isLoading ? (
        <Skeleton className="mt-3 h-8 w-full rounded-[8px]" />
      ) : (
        <div className="mt-3 flex items-center gap-2 border-t border-line pt-3">
          <RiskBadge level={overview.data?.riskLevel} scanned={(overview.data?.findings.length ?? 0) > 0} />
          <p className="text-xs leading-5 text-ink-secondary">
            {overviewFindings > 0
              ? t("toolbox.securityFindingsSummary", { n: overviewFindings })
              : t("toolbox.securityEmpty")}
          </p>
        </div>
      )}
      <div className="mt-3 space-y-1.5">
        {library.isLoading ? (
          [0, 1, 2].map((i) => <Skeleton key={i} className="h-10 w-full rounded-[8px]" />)
        ) : total === 0 ? (
          <p className="text-xs text-ink-faint">{t("toolbox.securityLibraryEmpty")}</p>
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
  const { t } = useI18n();
  const toolName = toolById.get(group.members[0]?.toolId ?? "")?.displayName ?? group.members[0]?.toolId ?? "";

  function deactivate(skillId: number, toolId: string, name: string) {
    revoke.mutate(
      { skillId, tools: [toolId], dryRun: false },
      {
        onSuccess: () => toast.show(t("toolbox.conflictDeactivatedToast", { skill: name, tool: toolName }), "success"),
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  return (
    <div className="rounded-[8px] border border-line bg-page p-2.5">
      <div className="flex items-center gap-1.5">
        <Badge tone="blue">{t("toolbox.conflictBadge")}</Badge>
        <span className="text-xs text-ink-secondary">
          {t("toolbox.conflictSimilarity", {
            tool: toolName,
            pct: (group.similarity * 100).toFixed(0),
          })}
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
              {t("toolbox.conflictDeactivate")}
            </Button>
          </div>
        ))}
      </div>
      <p className="mt-1.5 text-xs leading-4 text-ink-faint">{t("toolbox.conflictHint")}</p>
    </div>
  );
}

function CostSection() {
  const cost = useCostReport();
  const tools = useTools();
  const targets = useTargets();
  const { t } = useI18n();

  const toolById = new Map<string, ToolDto>((tools.data ?? []).map((x) => [x.id, x]));
  const perTool = cost.data?.perTool ?? [];
  const conflicts = cost.data?.conflicts ?? [];
  const totalSkills = perTool.reduce((s, x) => s + x.skillCount, 0);
  const totalTokens = perTool.reduce((s, x) => s + x.estimatedTokens, 0);
  const maxTokens = Math.max(1, ...perTool.map((x) => x.estimatedTokens));

  return (
    <section className="sb-card p-4">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-semibold text-ink">{t("toolbox.costTitle")}</h3>
        <Badge tone="neutral">{t("toolbox.costBadge")}</Badge>
      </div>
      {cost.isLoading ? (
        <Skeleton className="mt-3 h-24 w-full rounded-[8px]" />
      ) : (
        <>
          {/* 数字卡片 */}
          <div className="mt-3 grid grid-cols-2 gap-2 border-t border-line pt-3">
            <div className="rounded-[8px] bg-surface px-3 py-2.5">
              <div className="text-lg font-semibold text-ink">{totalSkills}</div>
              <div className="text-xs text-ink-faint">{t("toolbox.costTotalSkills")}</div>
            </div>
            <div className="rounded-[8px] bg-surface px-3 py-2.5">
              <div className="text-lg font-semibold text-ink">
                {totalTokens.toLocaleString()}
                <span className="ml-1 text-xs font-normal text-ink-faint">{t("toolbox.costTokensEst")}</span>
              </div>
              <div className="text-xs text-ink-faint">{t("toolbox.costDescCost")}</div>
            </div>
          </div>
          {/* 按工具分组的条形图（纯 CSS，无图表库） */}
          <div className="mt-3 space-y-2 border-t border-line pt-3">
            {perTool.length === 0 ? (
              <p className="text-xs text-ink-faint">{t("toolbox.costNoTools")}</p>
            ) : (
              perTool.map((x) => (
                <div key={x.toolId} className="flex items-center gap-2">
                  <span className="w-28 shrink-0 truncate text-xs text-ink-secondary" title={toolById.get(x.toolId)?.displayName ?? x.toolId}>
                    {toolById.get(x.toolId)?.displayName ?? x.toolId}
                  </span>
                  <div className="h-4 min-w-0 flex-1 overflow-hidden rounded-[4px] bg-line">
                    <div
                      className="h-full rounded-[4px] bg-primary transition-transform duration-200 ease-skillbox"
                      style={{ width: `${(x.estimatedTokens / maxTokens) * 100}%` }}
                    />
                  </div>
                  <span className="w-32 shrink-0 text-right text-xs text-ink-faint">
                    {t("toolbox.costPerTool", { n: x.skillCount, tokens: x.estimatedTokens.toLocaleString() })}
                  </span>
                </div>
              ))
            )}
          </div>
          {/* 冲突组 */}
          <div className="mt-3 border-t border-line pt-3">
            <h4 className="text-xs font-semibold text-ink">{t("toolbox.costConflictTitle")}</h4>
            <div className="mt-2 space-y-2">
              {conflicts.length === 0 ? (
                <p className="text-xs text-ink-faint">{t("toolbox.costNoConflicts")}</p>
              ) : (
                conflicts.map((g, i) => <ConflictCard key={i} group={g} toolById={toolById} />)
              )}
            </div>
          </div>
        </>
      )}
      {targets.isError && (
        <p className="mt-2 text-xs text-danger">{t("toolbox.targetsError", { msg: ipcError(targets.error) })}</p>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// 页面
// ---------------------------------------------------------------------------

export default function ToolboxPage() {
  const { t } = useI18n();
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-8 py-6">
      <div className="flex items-center gap-2">
        <h2 className="text-base font-semibold text-ink">{t("toolbox.title")}</h2>
        <p className="text-xs text-ink-faint">{t("toolbox.subtitle")}</p>
      </div>
      <div className="mt-4 grid max-w-5xl grid-cols-1 items-start gap-3 xl:grid-cols-2">
        <SecuritySection />
        <CostSection />
      </div>
      <div className="mt-3 max-w-5xl">
        <EmptyState title={t("toolbox.moreTitle")} description={t("toolbox.moreDesc")} />
      </div>
    </div>
  );
}
