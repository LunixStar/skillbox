/// 体检页（§6.2/§6.3）：doctor 只读报告按严重度分组 + 人话解释；
/// 逐项修复（先 dry-run 影响清单 → 逐个二次确认 → 执行）；
/// 危险项无批量入口；DuplicateName 只提供"查看差异"，绝不代选正本。

import { useMemo, useState } from "react";
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

const SEVERITY_ORDER: Severity[] = ["critical", "warning", "info"];

const SEVERITY_META: Record<Severity, { label: string; tone: "danger" | "blue" | "neutral" }> = {
  critical: { label: "严重问题", tone: "danger" },
  warning: { label: "需要注意", tone: "blue" },
  info: { label: "提示", tone: "neutral" },
};

/// 人话标题（不抛枚举名）：toolName 为空时省略工具前缀。
const KIND_TITLE: Record<DiscrepancyKind, string> = {
  unmanagedRealDir: "目录里有一个不是 SkillBox 管理的文件夹，它可能是你自己放的",
  brokenLink: "部署链接指向的位置已经不存在了（悬空链接）",
  foreignLink: "链接指向 SkillBox 中央库之外的地方，SkillBox 不会动它",
  missingTarget: "记录里显示已部署，但磁盘上找不到",
  unrecordedDeployment: "磁盘上有一个 SkillBox 风格的链接，但 SkillBox 没有它的记录",
  copyDrift: "副本内容与中央库正本不一致（可能被手动改过）",
  duplicateName: "多个目录里有同名但内容不同的 Skill",
};

type IssueAction = { label: string; danger: boolean } | { label: string; viewDiff: true } | null;

function issueAction(issue: DoctorIssue): IssueAction {
  const d = issue.discrepancy;
  switch (d.kind) {
    case "unmanagedRealDir":
      return { label: "收编到中央库", danger: true };
    case "foreignLink":
      return { label: "移除外来链接", danger: true };
    case "brokenLink":
      return d.suggestedFix === "rebuildLink"
        ? { label: "重建链接", danger: false }
        : { label: "移除悬空链接", danger: true };
    case "missingTarget":
      return { label: "重新部署", danger: false };
    case "unrecordedDeployment":
      return { label: "补登记", danger: false };
    case "copyDrift":
      return { label: "用正本更新副本", danger: true };
    case "duplicateName":
      return { label: "查看差异", viewDiff: true };
  }
}

function outcomeMessage(outcome: FixOutcome): { text: string; ok: boolean } {
  if (outcome === "applied") {
    return { text: "已修复", ok: true };
  }
  if ("skipped" in outcome) {
    return { text: `已跳过：${outcome.skipped.reason}`, ok: false };
  }
  if ("failed" in outcome) {
    return { text: `修复失败：${outcome.failed.reason}`, ok: false };
  }
  return { text: outcome.needsConfirmation.reason, ok: true };
}

export default function DoctorPage() {
  const report = useDoctorReport();
  const tools = useTools();
  const dryFix = useDoctorFix();
  const realFix = useDoctorFix();
  const openPath = useOpenPath();
  const toast = useToast();
  const qc = useQueryClient();

  const [pending, setPending] = useState<{ issue: DoctorIssue; impact: string } | null>(null);
  const [diffIssue, setDiffIssue] = useState<DoctorIssue | null>(null);

  const toolName = useMemo(() => {
    const m = new Map<string, string>();
    for (const t of tools.data ?? []) {
      m.set(t.id, t.displayName);
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

  function renderIssue(issue: DoctorIssue) {
    const d = issue.discrepancy;
    const tool = d.toolId ? (toolName.get(d.toolId) ?? d.toolId) : null;
    const action = issueAction(issue);
    const dangerous = action !== null && "danger" in action && action.danger;
    const paths = [d.path, ...d.related];
    return (
      <div key={issue.id} className="sb-card flex items-start gap-3 p-4">
        <div
          className={`mt-0.5 shrink-0 ${d.severity === "critical" ? "text-danger" : "text-primary"}`}
        >
          {d.severity === "critical" ? <ShieldAlert size={16} /> : <AlertTriangle size={16} />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-sm font-medium text-ink">
              {tool ? `「${tool}」` : ""}
              {KIND_TITLE[d.kind]}
            </span>
            {dangerous && <Badge tone="danger">危险项</Badge>}
          </div>
          <p className="mt-1 text-xs leading-5 text-ink-secondary">{d.detail}</p>
          <div className="mt-1.5 space-y-0.5">
            {paths.slice(0, 3).map((p) => (
              <div key={p} className="truncate font-mono text-xs text-ink-faint" title={p}>
                {p}
              </div>
            ))}
            {paths.length > 3 && (
              <div className="text-xs text-ink-faint">…及其他 {paths.length - 3} 个路径</div>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {action && "viewDiff" in action ? (
            <Button size="sm" onClick={() => setDiffIssue(issue)}>
              <Eye size={13} />
              {action.label}
            </Button>
          ) : action ? (
            <Button
              size="sm"
              variant={dangerous ? "danger" : "secondary"}
              disabled={dryFix.isPending || realFix.isPending}
              onClick={() => startFix(issue)}
            >
              {action.label}
            </Button>
          ) : null}
        </div>
      </div>
    );
  }

  // ------------------------------------------------------------------
  // 页面
  // ------------------------------------------------------------------
  const lastRun = report.dataUpdatedAt > 0 ? new Date(report.dataUpdatedAt).toLocaleString("zh-CN") : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-8 py-6">
      {/* 顶栏：上次体检时间 + 重新体检（doctor 只读，随时可安全重跑） */}
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="text-base font-semibold text-ink">体检</h2>
          <p className="mt-0.5 text-xs text-ink-faint">
            {lastRun ? `上次体检：${lastRun}` : "尚未体检"}
            {report.data
              ? ` · 已检查 ${report.data.checkedTools.length} 个工具 · 耗时 ${report.data.durationMs} ms`
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
          {report.isFetching ? "体检中…" : "重新体检"}
        </Button>
      </div>

      <div className="mt-4 min-h-0 flex-1 space-y-5 overflow-y-auto pr-0.5">
        {report.isError ? (
          <EmptyState
            title="体检失败"
            description={ipcError(report.error)}
            action={
              <Button variant="primary" size="sm" onClick={() => void report.refetch()}>
                重试
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
            title="体检通过"
            description="中央库与各工具的部署状态一致，没有发现需要处理的问题。"
          />
        ) : (
          SEVERITY_ORDER.map((sev) => {
            const issues = grouped.get(sev) ?? [];
            if (issues.length === 0) {
              return null;
            }
            const meta = SEVERITY_META[sev];
            return (
              <section key={sev}>
                <div className="mb-2 flex items-center gap-2">
                  {sev === "info" ? <Info size={14} className="text-ink-faint" /> : null}
                  <Badge tone={meta.tone} dot>
                    {meta.label}
                  </Badge>
                  <span className="text-xs text-ink-faint">{issues.length} 项</span>
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
        title={pending ? `确认${issueAction(pending.issue)?.label ?? "修复"}` : ""}
        footer={
          <>
            <Button onClick={() => setPending(null)}>再想想</Button>
            <Button variant="danger" disabled={realFix.isPending} onClick={confirmFix}>
              确认执行
            </Button>
          </>
        }
      >
        {pending && (
          <>
            <p className="whitespace-pre-wrap">{pending.impact}</p>
            <div className="mt-3 space-y-1 border-t border-line pt-2.5">
              <p className="text-xs text-ink-faint">将影响以下路径：</p>
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
        title="查看差异"
        footer={
          <>
            <Button
              onClick={() =>
                openPath.mutate("central", { onError: (e) => toast.show(ipcError(e), "danger") })
              }
            >
              <FolderOpen size={13} />
              打开中央库
            </Button>
            <Button variant="primary" onClick={() => setDiffIssue(null)}>
              关闭
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
            <p className="mt-3 text-xs text-ink-faint">
              同名目录需要你自己决定以哪个为准：SkillBox 不会自动挑选正本，也不会改动这些目录。
              可以在确认想保留的版本后，手动删除或改名其他版本。
            </p>
          </>
        )}
      </Modal>
    </div>
  );
}
