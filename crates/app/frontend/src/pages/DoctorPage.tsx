/// 体检页（§6.2/§6.3）：doctor 只读报告按严重度分组 + 人话解释；
/// 逐项修复（先 dry-run 影响清单 → 逐个二次确认 → 执行）；
/// 危险项无批量入口；DuplicateName 只提供"查看差异"，绝不代选正本。

import { useMemo, useState, type CSSProperties } from "react";
import { AlertTriangle, CheckCircle2, Eye, FolderOpen, Info, RefreshCw, ShieldAlert } from "lucide-react";
import {
  ipcError,
  type DoctorIssue,
  type DiscrepancyKind,
  type FixOutcome,
  type Severity,
} from "../api/ipc";
import { queryKeys, useDoctorFix, useDoctorReport, useOpenPath, useTools } from "../api/queries";
import { useToast } from "../components/Toast";
import { Badge, Button, EmptyState, Modal, Skeleton } from "../components/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useI18n } from "../i18n";

const SEVERITY_ORDER: Severity[] = ["critical", "warning", "info"];

const SEVERITY_LABEL_KEY: Record<Severity, string> = {
  critical: "doctor.severityCritical",
  warning: "doctor.severityWarning",
  info: "doctor.severityInfo",
};

const SEVERITY_TONE: Record<Severity, "danger" | "blue" | "neutral"> = {
  critical: "danger",
  warning: "blue",
  info: "neutral",
};

/// 人话标题 key（不抛枚举名）。
const KIND_TITLE_KEY: Record<DiscrepancyKind, string> = {
  unmanagedRealDir: "doctor.kind.unmanagedRealDir",
  brokenLink: "doctor.kind.brokenLink",
  foreignLink: "doctor.kind.foreignLink",
  missingTarget: "doctor.kind.missingTarget",
  unrecordedDeployment: "doctor.kind.unrecordedDeployment",
  copyDrift: "doctor.kind.copyDrift",
  duplicateName: "doctor.kind.duplicateName",
};

type IssueAction = { labelKey: string; danger: boolean } | { labelKey: string; viewDiff: true } | null;

function issueAction(issue: DoctorIssue): IssueAction {
  const d = issue.discrepancy;
  switch (d.kind) {
    case "unmanagedRealDir":
      return { labelKey: "doctor.action.adopt", danger: true };
    case "foreignLink":
      return { labelKey: "doctor.action.removeForeign", danger: true };
    case "brokenLink":
      return d.suggestedFix === "rebuildLink"
        ? { labelKey: "doctor.action.rebuild", danger: false }
        : { labelKey: "doctor.action.removeBroken", danger: true };
    case "missingTarget":
      return { labelKey: "doctor.action.redeploy", danger: false };
    case "unrecordedDeployment":
      return { labelKey: "doctor.action.record", danger: false };
    case "copyDrift":
      return { labelKey: "doctor.action.updateCopy", danger: true };
    case "duplicateName":
      return { labelKey: "doctor.action.viewDiff", viewDiff: true };
  }
}

export default function DoctorPage() {
  const report = useDoctorReport();
  const tools = useTools();
  const dryFix = useDoctorFix();
  const realFix = useDoctorFix();
  const openPath = useOpenPath();
  const toast = useToast();
  const qc = useQueryClient();
  const { t, lang } = useI18n();

  const [pending, setPending] = useState<{ issue: DoctorIssue; impact: string } | null>(null);
  const [diffIssue, setDiffIssue] = useState<DoctorIssue | null>(null);

  const toolName = useMemo(() => {
    const m = new Map<string, string>();
    for (const x of tools.data ?? []) {
      m.set(x.id, x.displayName);
    }
    return m;
  }, [tools.data]);

  const grouped = useMemo(() => {
    const by = new Map<Severity, DoctorIssue[]>();
    for (const issue of report.data?.issues ?? []) {
      const list = by.get(issue.severity) ?? [];
      list.push(issue);
      by.set(issue.severity, list);
    }
    return by;
  }, [report.data]);

  function outcomeMessage(outcome: FixOutcome): { text: string; ok: boolean } {
    if (outcome === "applied") {
      return { text: t("doctor.fixApplied"), ok: true };
    }
    if ("skipped" in outcome) {
      return { text: t("doctor.fixSkipped", { reason: outcome.skipped.reason }), ok: false };
    }
    if ("failed" in outcome) {
      return { text: t("doctor.fixFailed", { reason: outcome.failed.reason }), ok: false };
    }
    return { text: outcome.needsConfirmation.reason, ok: true };
  }

  // ------------------------------------------------------------------
  // 修复流：dry-run 影响清单 → 逐个二次确认 → 执行（§6.3）。
  // ------------------------------------------------------------------
  function startFix(issue: DoctorIssue) {
    dryFix.mutate(
      { items: [issue.id], confirmed: false, dryRun: true },
      {
        onSuccess: (rep) => {
          const result = rep.results[0];
          if (result && typeof result.outcome === "object" && "needsConfirmation" in result.outcome) {
            setPending({ issue, impact: result.outcome.needsConfirmation.reason });
          } else if (result) {
            const m = outcomeMessage(result.outcome);
            toast.show(m.text, m.ok ? "success" : "danger");
          }
        },
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  function confirmFix() {
    const p = pending;
    if (!p) {
      return;
    }
    realFix.mutate(
      { items: [p.issue.id], confirmed: true, dryRun: false },
      {
        onSuccess: (rep) => {
          const result = rep.results[0];
          if (result) {
            const m = outcomeMessage(result.outcome);
            toast.show(m.text, m.ok ? "success" : "danger");
          }
          void qc.invalidateQueries({ queryKey: queryKeys.doctor });
        },
        onError: (e) => toast.show(ipcError(e), "danger"),
        onSettled: () => setPending(null),
      },
    );
  }

  function renderIssue(issue: DoctorIssue, i: number) {
    const d = issue.discrepancy;
    const tool = d.toolId ? (toolName.get(d.toolId) ?? d.toolId) : null;
    const action = issueAction(issue);
    const dangerous = action !== null && "danger" in action && action.danger;
    const paths = [d.path, ...d.related];
    return (
      <div key={issue.id} className="sb-card sb-fade-up flex items-start gap-3 p-4" style={{ "--sb-i": i } as CSSProperties}>
        <div
          className={`mt-0.5 shrink-0 ${d.severity === "critical" ? "text-danger" : "text-primary"}`}
        >
          {d.severity === "critical" ? <ShieldAlert size={16} /> : <AlertTriangle size={16} />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-sm font-medium text-ink">
              {tool ? `「${tool}」` : ""}
              {t(KIND_TITLE_KEY[d.kind])}
            </span>
            {dangerous && <Badge tone="danger">{t("doctor.dangerBadge")}</Badge>}
          </div>
          <p className="mt-1 text-xs leading-5 text-ink-secondary">{d.detail}</p>
          <div className="mt-1.5 space-y-0.5">
            {paths.slice(0, 3).map((p) => (
              <div key={p} className="truncate font-mono text-xs text-ink-faint" title={p}>
                {p}
              </div>
            ))}
            {paths.length > 3 && (
              <div className="text-xs text-ink-faint">{t("doctor.morePaths", { n: paths.length - 3 })}</div>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {action && "viewDiff" in action ? (
            <Button size="sm" onClick={() => setDiffIssue(issue)}>
              <Eye size={13} />
              {t(action.labelKey)}
            </Button>
          ) : action ? (
            <Button
              size="sm"
              variant={dangerous ? "danger" : "secondary"}
              disabled={dryFix.isPending || realFix.isPending}
              onClick={() => startFix(issue)}
            >
              {t(action.labelKey)}
            </Button>
          ) : null}
        </div>
      </div>
    );
  }

  // ------------------------------------------------------------------
  // 页面
  // ------------------------------------------------------------------
  const locale = lang === "en" ? "en-US" : "zh-CN";
  const lastRun = report.dataUpdatedAt > 0 ? new Date(report.dataUpdatedAt).toLocaleString(locale) : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-8 py-6">
      {/* 顶栏：上次体检时间 + 重新体检（doctor 只读，随时可安全重跑） */}
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="text-base font-semibold text-ink">{t("doctor.title")}</h2>
          <p className="mt-0.5 text-xs text-ink-faint">
            {lastRun ? t("doctor.lastRun", { time: lastRun }) : t("doctor.neverRun")}
            {report.data
              ? t("doctor.checkedSuffix", { n: report.data.checkedTools.length, ms: report.data.durationMs })
              : ""}
          </p>
        </div>
        <Button
          variant="primary"
          size="sm"
          disabled={report.isFetching}
          onClick={() => void report.refetch()}
        >
          <RefreshCw size={13} className={report.isFetching ? "animate-spin" : ""} />
          {report.isFetching ? t("doctor.running") : t("doctor.rerun")}
        </Button>
      </div>

      <div className="mt-4 min-h-0 flex-1 space-y-5 overflow-y-auto pr-0.5">
        {report.isError ? (
          <EmptyState
            title={t("doctor.errorTitle")}
            description={ipcError(report.error)}
            action={
              <Button variant="primary" size="sm" onClick={() => void report.refetch()}>
                {t("common.retry")}
              </Button>
            }
          />
        ) : report.isLoading ? (
          <div className="space-y-2.5">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-20 w-full rounded-card" />
            ))}
          </div>
        ) : (report.data?.issues ?? []).length === 0 ? (
          <EmptyState
            icon={<CheckCircle2 size={22} />}
            title={t("doctor.allClearTitle")}
            description={t("doctor.allClearDesc")}
          />
        ) : (
          SEVERITY_ORDER.map((sev) => {
            const issues = grouped.get(sev) ?? [];
            if (issues.length === 0) {
              return null;
            }
            return (
              <section key={sev}>
                <div className="mb-2 flex items-center gap-2">
                  {sev === "info" ? <Info size={14} className="text-ink-faint" /> : null}
                  <Badge tone={SEVERITY_TONE[sev]} dot>
                    {t(SEVERITY_LABEL_KEY[sev])}
                  </Badge>
                  <span className="text-xs text-ink-faint">{t("doctor.issueCount", { n: issues.length })}</span>
                </div>
                <div className="space-y-2.5">{issues.map(renderIssue)}</div>
              </section>
            );
          })
        )}
      </div>

      {/* 影响清单确认（修复前可见；危险项逐个二次确认，无批量入口） */}
      <Modal
        open={pending !== null}
        onClose={() => setPending(null)}
        title={
          pending
            ? t("doctor.confirmTitlePrefix", {
                action: issueAction(pending.issue)?.labelKey ? t(issueAction(pending.issue)!.labelKey) : "",
              })
            : ""
        }
        footer={
          <>
            <Button onClick={() => setPending(null)}>{t("common.holdOn")}</Button>
            <Button variant="danger" disabled={realFix.isPending} onClick={confirmFix}>
              {t("doctor.confirmRun")}
            </Button>
          </>
        }
      >
        {pending && (
          <>
            <p className="whitespace-pre-wrap">{pending.impact}</p>
            <div className="mt-3 space-y-1 border-t border-line pt-2.5">
              <p className="text-xs text-ink-faint">{t("doctor.impactPaths")}</p>
              {[pending.issue.discrepancy.path, ...pending.issue.discrepancy.related].map((p) => (
                <div key={p} className="truncate font-mono text-xs text-ink-secondary" title={p}>
                  {p}
                </div>
              ))}
            </div>
          </>
        )}
      </Modal>

      {/* DuplicateName：只查看差异，SkillBox 绝不代选正本 */}
      <Modal
        open={diffIssue !== null}
        onClose={() => setDiffIssue(null)}
        title={t("doctor.viewDiffTitle")}
        footer={
          <>
            <Button
              onClick={() =>
                openPath.mutate("central", { onError: (e) => toast.show(ipcError(e), "danger") })
              }
            >
              <FolderOpen size={13} />
              {t("common.openCentralLibrary")}
            </Button>
            <Button variant="primary" onClick={() => setDiffIssue(null)}>
              {t("common.close")}
            </Button>
          </>
        }
      >
        {diffIssue && (
          <>
            <p className="whitespace-pre-wrap">{diffIssue.discrepancy.detail}</p>
            <div className="mt-3 space-y-1">
              {[diffIssue.discrepancy.path, ...diffIssue.discrepancy.related].map((p) => (
                <div key={p} className="truncate rounded-[6px] bg-surface px-2 py-1 font-mono text-xs text-ink-secondary" title={p}>
                  {p}
                </div>
              ))}
            </div>
            <p className="mt-3 text-xs text-ink-faint">{t("doctor.viewDiffHint")}</p>
          </>
        )}
      </Modal>
    </div>
  );
}
