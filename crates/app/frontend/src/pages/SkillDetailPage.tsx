/// Skill 详情页（§9.4 + §9.3）：SKILL.md 预览/编辑（CodeMirror，校验行内
/// 标记 + 底部问题列表，错误阻断保存）· 来源溯源 · 更新 · 部署状态。
/// 更新语义按来源分派：git=fetch 比对（先看 diff 摘要再应用）、
/// local=重新导入比对、archive=不支持在线更新（诚实提示）。

import { useEffect, useState, type CSSProperties } from "react";
import {
  ArrowLeft,
  ExternalLink,
  FolderOpen,
  Pencil,
  RefreshCw,
  Save,
  Unlink,
  X,
} from "lucide-react";
import {
  ipcError,
  isReadOnlySkill,
  type DocCheckReport,
  type SkillMeta,
  type TargetRow,
  type ToolDto,
  type UpdateCheckOutcome,
  type UpdateOutcome,
} from "../api/ipc";
import {
  docValidate,
  queryKeys,
  useDeployApply,
  useDeployRevoke,
  useDocSave,
  useOpenPath,
  useSecurityReport,
  useSkillDoc,
  useTargets,
  useTools,
  useUpdateApply,
  useUpdateCheck,
} from "../api/queries";
import DocEditor from "../components/DocEditor";
import { DocIssueList } from "../components/DocIssueList";
import { Markdown } from "../components/Markdown";
import { FindingList, RiskBadge } from "../components/SecurityKit";
import { useToast } from "../components/Toast";
import { Badge, Button, EmptyState, Modal, Skeleton } from "../components/ui";
import { useI18n } from "../i18n";
import { useQueryClient } from "@tanstack/react-query";

const ORIGIN_LABEL_KEY: Record<SkillMeta["originKind"], string> = {
  local: "common.origin.local",
  git: "common.origin.git",
  archive: "common.origin.archive",
};

const MODE_LABEL_KEY: Record<TargetRow["mode"], string> = {
  symlink: "detail.modeSymlink",
  copy: "detail.modeCopy",
};

const ORIGIN_REF_LABEL_KEY: Record<SkillMeta["originKind"], string> = {
  local: "detail.originPath",
  git: "detail.originRepo",
  archive: "detail.originArchive",
};

interface SkillDetailPageProps {
  skill: SkillMeta;
  onBack: () => void;
}

export default function SkillDetailPage({ skill, onBack }: SkillDetailPageProps) {
  const tools = useTools();
  const targets = useTargets();
  const doc = useSkillDoc(skill.id);
  const security = useSecurityReport(skill.id);
  const check = useUpdateCheck();
  const apply = useUpdateApply();
  const deployApply = useDeployApply();
  const deployRevoke = useDeployRevoke();
  const docSave = useDocSave();
  const openPath = useOpenPath();
  const toast = useToast();
  const qc = useQueryClient();
  const { t } = useI18n();

  const [checkResult, setCheckResult] = useState<UpdateCheckOutcome | null>(null);
  const [showApplyConfirm, setShowApplyConfirm] = useState(false);

  // ------------------------------------------------------------------
  // §9.3 编辑器：草稿 + 防抖实时校验（错误阻断保存、告警不阻断）
  // ------------------------------------------------------------------
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [report, setReport] = useState<DocCheckReport | null>(null);
  const [validating, setValidating] = useState(false);
  /// 丢弃未保存草稿前的确认（取消编辑 / 返回列表都会触发）。
  const [discardAction, setDiscardAction] = useState<null | (() => void)>(null);

  const savedText = doc.data ?? "";
  const dirty = editing && draft !== savedText;
  const hasErrors = (report?.errors.length ?? 0) > 0;

  /// 防抖 350ms 实时校验（零写盘；失败静默不标红——保存时后端仍是权威闸门）。
  useEffect(() => {
    if (!editing) {
      return;
    }
    setValidating(true);
    const timer = setTimeout(() => {
      docValidate(skill.id, draft)
        .then((r) => setReport(r))
        .catch(() => setReport(null))
        .finally(() => setValidating(false));
    }, 350);
    return () => clearTimeout(timer);
  }, [editing, draft, skill.id]);

  function startEdit() {
    setDraft(savedText);
    setReport(null);
    setEditing(true);
  }

  function exitEdit() {
    setEditing(false);
    setReport(null);
  }

  /// 取消编辑 / 返回：草稿有未保存改动先确认，避免静默丢失。
  function requestDiscard(action: () => void) {
    if (dirty) {
      setDiscardAction(() => action);
    } else {
      action();
    }
  }

  function saveDoc() {
    docSave.mutate(
      { skillId: skill.id, contents: draft },
      {
        onSuccess: (outcome) => {
          exitEdit();
          if (outcome.indexError) {
            toast.show(t("detail.saveIndexError", { msg: outcome.indexError }), "danger");
          } else {
            toast.show(t("detail.saveOk"), "success");
          }
        },
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  const readOnly = isReadOnlySkill(skill.canonicalPath, tools.data ?? []);
  const toolById = new Map<string, ToolDto>((tools.data ?? []).map((x) => [x.id, x]));
  const myTargets = (targets.data ?? []).filter((x) => x.skillId === skill.id);
  const deployedToolIds = new Set(myTargets.map((x) => x.toolId));
  const deployableTools = (tools.data ?? []).filter((x) => x.enabled && x.installed);

  // ------------------------------------------------------------------
  // 更新：检查 → diff 摘要确认 → 应用
  // ------------------------------------------------------------------
  function runCheck() {
    setCheckResult(null);
    check.mutate(skill.id, {
      onSuccess: (outcome) => setCheckResult(outcome),
      onError: (e) => toast.show(ipcError(e), "danger"),
    });
  }

  function runApply() {
    setShowApplyConfirm(false);
    apply.mutate(
      { skillId: skill.id, dryRun: false },
      {
        onSuccess: (outcome: UpdateOutcome) => {
          if (outcome === "upToDate") {
            toast.show(t("detail.updateUpToDateToast"), "success");
          } else if ("applied" in outcome) {
            toast.show(t("detail.updateAppliedToast", { rev: shortRevision(outcome.applied.revision) }), "success");
          } else if ("unsupported" in outcome) {
            toast.show(outcome.unsupported.reason, "danger");
          }
          setCheckResult(null);
        },
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  // ------------------------------------------------------------------
  // 部署 / 取消部署（只读来源禁用，后端另有硬守卫）
  // ------------------------------------------------------------------
  function deployTo(toolId: string) {
    deployApply.mutate(
      { skillId: skill.id, tools: [toolId], mode: "symlink", overwrite: false, dryRun: false },
      {
        onSuccess: () => toast.show(t("common.deployToast", { tool: toolById.get(toolId)?.displayName ?? toolId }), "success"),
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  function revokeFrom(toolId: string) {
    deployRevoke.mutate(
      { skillId: skill.id, tools: [toolId], dryRun: false },
      {
        onSuccess: () => toast.show(t("common.revokeToast", { tool: toolById.get(toolId)?.displayName ?? toolId }), "success"),
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  function openSourceUrl() {
    if (skill.originRef) {
      openPath.mutate(skill.originRef, { onError: (e) => toast.show(ipcError(e), "danger") });
    }
  }

  function openCanonicalDir() {
    openPath.mutate(`skill:${skill.id}`, { onError: (e) => toast.show(ipcError(e), "danger") });
  }

  /// 缩写 revision（+ 「未记录」i18n）。
  function shortRevision(rev: string | null): string {
    if (!rev) {
      return t("detail.revisionNone");
    }
    return rev.length > 12 ? `${rev.slice(0, 12)}…` : rev;
  }

  // ------------------------------------------------------------------
  // 渲染
  // ------------------------------------------------------------------
  /// 安全体检（§9.1）：打开详情页即扫描（纯静态，只提示不阻断安装）。
  function renderSecurityCard() {
    return (
      <section className="sb-card p-4">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold text-ink">{t("detail.securityTitle")}</h3>
          {security.isLoading ? (
            <Skeleton className="h-5 w-14 rounded-[6px]" />
          ) : (
            <RiskBadge level={security.data?.riskLevel} scanned={!!security.data} />
          )}
        </div>
        <div className="mt-2">
          {security.isLoading ? (
            <div className="space-y-2">
              {[0, 1].map((i) => (
                <Skeleton key={i} className="h-10 w-full rounded-[8px]" />
              ))}
            </div>
          ) : security.isError ? (
            <p className="text-xs leading-5 text-danger">{t("detail.securityFail", { msg: ipcError(security.error) })}</p>
          ) : (
            <FindingList findings={security.data?.findings ?? []} />
          )}
        </div>
      </section>
    );
  }

  function renderUpdateCard() {
    return (
      <section className="sb-card p-4">
        <h3 className="text-sm font-semibold text-ink">{t("detail.updateTitle")}</h3>
        {skill.originKind === "archive" ? (
          <p className="mt-2 text-xs leading-5 text-ink-secondary">{t("detail.updateArchiveNoSupport")}</p>
        ) : (
          <>
            <div className="mt-2 flex items-center gap-2">
              <Button variant="secondary" size="sm" disabled={check.isPending} onClick={runCheck}>
                <RefreshCw size={13} className={check.isPending ? "animate-spin" : ""} />
                {t(skill.originKind === "git" ? "detail.updateCheckGit" : "detail.updateCheckLocal")}
              </Button>
            </div>
            {check.isPending && <Skeleton className="mt-3 h-12 w-full rounded-[8px]" />}
            {checkResult && (
              <div className="mt-3 space-y-2 border-t border-line pt-3">
                {checkResult.status === "upToDate" && (
                  <p className="text-xs leading-5 text-success">
                    {t("detail.updateUpToDate")}
                    {checkResult.note ? t("detail.updateNoteSuffix", { note: checkResult.note }) : ""}
                  </p>
                )}
                {checkResult.status === "unsupported" && (
                  <p className="text-xs leading-5 text-ink-secondary">
                    {checkResult.note ?? t("detail.updateUnsupportedDefault")}
                  </p>
                )}
                {checkResult.status === "updateAvailable" && (
                  <>
                    <p className="text-xs leading-5 text-ink">
                      {t(skill.originKind === "git" ? "detail.updateGitNew" : "detail.updateLocalNew")}
                      <span className="font-mono">{shortRevision(checkResult.currentRevision)}</span>
                      {" → "}
                      <span className="font-mono">{shortRevision(checkResult.newRevision)}</span>
                    </p>
                    {checkResult.diffSummary && (
                      <pre className="max-h-36 overflow-y-auto whitespace-pre-wrap rounded-[8px] bg-surface px-2.5 py-2 font-mono text-xs leading-5 text-ink-secondary">
                        {checkResult.diffSummary}
                      </pre>
                    )}
                    <Button variant="primary" size="sm" disabled={apply.isPending} onClick={() => setShowApplyConfirm(true)}>
                      {t("detail.updateApply")}
                    </Button>
                  </>
                )}
              </div>
            )}
          </>
        )}
      </section>
    );
  }

  function renderDeployCard() {
    if (readOnly) {
      return (
        <section className="sb-card p-4">
          <h3 className="text-sm font-semibold text-ink">{t("detail.deployTitle")}</h3>
          <div className="mt-2 flex items-center gap-2">
            <Badge tone="danger" dot>{t("detail.deployReadOnlyBadge")}</Badge>
            <p className="text-xs leading-5 text-ink-secondary">{t("detail.deployReadOnlyDesc")}</p>
          </div>
        </section>
      );
    }
    return (
      <section className="sb-card p-4">
        <h3 className="text-sm font-semibold text-ink">{t("detail.deployTitle")}</h3>
        {myTargets.length > 0 ? (
          <div className="mt-2 space-y-2">
            {myTargets.map((x) => (
              <div key={x.toolId} className="flex items-center gap-2 rounded-[8px] bg-surface px-2.5 py-2">
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5 text-xs font-medium text-ink">
                    {toolById.get(x.toolId)?.displayName ?? x.toolId}
                    <Badge tone="blue">{t(MODE_LABEL_KEY[x.mode])}</Badge>
                  </span>
                  <span className="mt-0.5 block truncate font-mono text-xs text-ink-faint" title={x.path}>
                    {x.path}
                  </span>
                </span>
                <Button variant="ghost" size="sm" disabled={deployRevoke.isPending} onClick={() => revokeFrom(x.toolId)}>
                  <Unlink size={13} />
                  {t("common.cancel")}
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <p className="mt-2 text-xs text-ink-faint">{t("detail.deployNone")}</p>
        )}
        {deployableTools.filter((x) => !deployedToolIds.has(x.id)).length > 0 && (
          <div className="mt-3 flex flex-wrap items-center gap-1.5 border-t border-line pt-3">
            <span className="text-xs text-ink-faint">{t("detail.deployTo")}</span>
            {deployableTools
              .filter((x) => !deployedToolIds.has(x.id))
              .map((x) => (
                <Button key={x.id} size="sm" disabled={deployApply.isPending} onClick={() => deployTo(x.id)}>
                  {(x.icon || x.displayName).slice(0, 2)} {x.displayName}
                </Button>
              ))}
          </div>
        )}
      </section>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col px-8 py-6">
      {/* 头部：返回 · 名称 · 徽章（来源 / 安全 / 只读） */}
      <div className="flex items-start gap-3">
        <Button variant="ghost" size="sm" onClick={() => requestDiscard(onBack)}>
          <ArrowLeft size={14} />
          {t("common.back")}
        </Button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h2 className="truncate text-base font-semibold text-ink">{skill.name}</h2>
            <Badge tone={skill.originKind === "local" ? "neutral" : "blue"}>{t(ORIGIN_LABEL_KEY[skill.originKind])}</Badge>
            <RiskBadge level={security.data?.riskLevel} scanned={!!security.data} />
            {readOnly && <Badge tone="danger" dot>{t("detail.deployReadOnlyBadge")}</Badge>}
          </div>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-ink-secondary">
            {skill.description || t("common.noDescription")}
          </p>
        </div>
      </div>

      <div className="mt-4 grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_360px] gap-3 overflow-hidden">
        {/* 左列：SKILL.md 预览 / 编辑（§9.3） */}
        <section
          className="sb-card sb-fade-up flex min-h-0 flex-col overflow-hidden p-0"
          style={{ "--sb-i": 0 } as CSSProperties}
        >
          <div className="flex items-center gap-2 border-b border-line px-4 py-2.5">
            <h3 className="flex-1 text-sm font-semibold text-ink">
              SKILL.md {editing ? t("detail.docEdit") : t("detail.docPreview")}
            </h3>
            {editing ? (
              <>
                <span className="text-xs text-ink-faint">
                  {validating
                    ? t("detail.editHintValidating")
                    : hasErrors
                      ? t("detail.editHintErrors")
                      : t("detail.editHintNormal")}
                </span>
                <Button size="sm" disabled={docSave.isPending} onClick={() => requestDiscard(exitEdit)}>
                  <X size={13} />
                  {t("common.cancel")}
                </Button>
                <Button variant="primary" size="sm" disabled={docSave.isPending || hasErrors} onClick={saveDoc}>
                  <Save size={13} />
                  {t("common.save")}
                </Button>
              </>
            ) : (
              <Button variant="secondary" size="sm" disabled={doc.isLoading || doc.isError} onClick={startEdit}>
                <Pencil size={13} />
                {t("common.edit")}
              </Button>
            )}
          </div>
          {editing ? (
            <>
              <div className="min-h-0 flex-1 overflow-hidden">
                <DocEditor
                  value={draft}
                  onChange={setDraft}
                  errors={report?.errors ?? []}
                  warnings={report?.warnings ?? []}
                />
              </div>
              {/* 底部问题列表：错误（红，阻断保存）+ 告警（蓝，不阻断） */}
              <div className="max-h-40 shrink-0 overflow-y-auto border-t border-line px-4 py-2.5">
                <DocIssueList validating={validating} report={report} />
              </div>
            </>
          ) : (
            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
              {doc.isLoading ? (
                <div className="space-y-2.5">
                  {[0, 1, 2, 3, 4].map((i) => (
                    <Skeleton key={i} className="h-4 w-full rounded-[4px]" />
                  ))}
                </div>
              ) : doc.isError ? (
                <EmptyState
                  title={t("detail.readDocFail")}
                  description={ipcError(doc.error)}
                  action={
                    <Button variant="primary" size="sm" onClick={() => void qc.invalidateQueries({ queryKey: queryKeys.skillDoc(skill.id) })}>
                      {t("common.retry")}
                    </Button>
                  }
                />
              ) : (
                <Markdown source={doc.data ?? ""} />
              )}
            </div>
          )}
        </section>

        {/* 右列：溯源 · 更新 · 部署状态 */}
        <div className="sb-fade-up flex min-h-0 flex-col gap-3 overflow-y-auto pr-0.5" style={{ "--sb-i": 1 } as CSSProperties}>
          <section className="sb-card p-4">
            <h3 className="text-sm font-semibold text-ink">{t("detail.originTitle")}</h3>
            <dl className="mt-2 space-y-2 text-xs leading-5">
              <div className="flex gap-2">
                <dt className="w-14 shrink-0 text-ink-faint">{t("detail.originType")}</dt>
                <dd className="min-w-0 text-ink">{t(ORIGIN_LABEL_KEY[skill.originKind])}</dd>
              </div>
              <div className="flex gap-2">
                <dt className="w-14 shrink-0 text-ink-faint">{t(ORIGIN_REF_LABEL_KEY[skill.originKind])}</dt>
                <dd className="min-w-0 flex-1">
                  <span className="block truncate font-mono text-ink" title={skill.originRef ?? skill.canonicalPath}>
                    {skill.originRef ?? skill.canonicalPath}
                  </span>
                </dd>
              </div>
              {skill.originKind === "git" && skill.originSubpath && (
                <div className="flex gap-2">
                  <dt className="w-14 shrink-0 text-ink-faint">{t("detail.originSubpath")}</dt>
                  <dd className="min-w-0 truncate font-mono text-ink" title={skill.originSubpath}>
                    {skill.originSubpath}
                  </dd>
                </div>
              )}
              {skill.originKind === "git" && (
                <div className="flex gap-2">
                  <dt className="w-14 shrink-0 text-ink-faint">{t("detail.originRevision")}</dt>
                  <dd className="min-w-0 truncate font-mono text-ink" title={skill.originRevision ?? undefined}>
                    {shortRevision(skill.originRevision)}
                  </dd>
                </div>
              )}
            </dl>
            <div className="mt-3 flex items-center gap-1.5 border-t border-line pt-2.5">
              {skill.originKind === "git" && skill.originRef && (
                <Button variant="secondary" size="sm" disabled={openPath.isPending} onClick={openSourceUrl}>
                  <ExternalLink size={13} />
                  {t("detail.openRepo")}
                </Button>
              )}
              <Button variant="secondary" size="sm" disabled={openPath.isPending} onClick={openCanonicalDir}>
                <FolderOpen size={13} />
                {t("detail.openDir")}
              </Button>
            </div>
          </section>

          {renderUpdateCard()}
          {renderSecurityCard()}
          {renderDeployCard()}
        </div>
      </div>

      {/* 应用更新前确认：diff 摘要已展示，此处明确「旧版进回收站」 */}
      <Modal
        open={showApplyConfirm}
        onClose={() => setShowApplyConfirm(false)}
        title={t("detail.applyConfirmTitle")}
        footer={
          <>
            <Button onClick={() => setShowApplyConfirm(false)}>{t("common.holdOn")}</Button>
            <Button variant="primary" disabled={apply.isPending} onClick={runApply}>
              {t("detail.applyConfirmOk")}
            </Button>
          </>
        }
      >
        <p>
          {t("detail.applyConfirmBody", { skill: skill.name })}
          {checkResult?.diffSummary ? t("detail.applyConfirmDiffNote") : ""}
        </p>
      </Modal>

      {/* 丢弃未保存草稿确认（取消编辑 / 返回列表） */}
      <Modal
        open={discardAction !== null}
        onClose={() => setDiscardAction(null)}
        title={t("detail.discardTitle")}
        footer={
          <>
            <Button onClick={() => setDiscardAction(null)}>{t("detail.discardKeep")}</Button>
            <Button
              variant="danger"
              onClick={() => {
                discardAction?.();
                setDiscardAction(null);
              }}
            >
              {t("detail.discardOk")}
            </Button>
          </>
        }
      >
        <p>{t("detail.discardBody")}</p>
      </Modal>
    </div>
  );
}
