/// Skill 详情页（§9.4 + §9.3）：SKILL.md 预览/编辑（CodeMirror，校验行内
/// 标记 + 底部问题列表，错误阻断保存）· 来源溯源 · 更新 · 部署状态。
/// 更新语义按来源分派：git=fetch 比对（先看 diff 摘要再应用）、
/// local=重新导入比对、archive=不支持在线更新（诚实提示）。

import { useEffect, useState } from "react";
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
  type DocIssue,
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
import { Markdown } from "../components/Markdown";
import { FindingList, RiskBadge } from "../components/SecurityKit";
import { useToast } from "../components/Toast";
import { Badge, Button, EmptyState, Modal, Skeleton } from "../components/ui";
import { useQueryClient } from "@tanstack/react-query";

const ORIGIN_LABEL: Record<SkillMeta["originKind"], string> = {
  local: "本地",
  git: "Git",
  archive: "压缩包",
};

const MODE_LABEL: Record<TargetRow["mode"], string> = {
  symlink: "链接",
  copy: "副本",
};

/// §9.3 校验规则人话标签（底部问题列表用）。
const DOC_RULE_LABEL: Record<string, string> = {
  missing_name: "缺少 name",
  invalid_name: "name 不合法",
  name_mismatch: "name 与登记名不一致",
  missing_description: "缺少 description",
  short_description: "description 过短",
  long_description: "description 过长",
  unclosed_fence: "代码块未闭合",
  missing_file: "引用文件缺失",
};

function shortRevision(rev: string | null): string {
  if (!rev) {
    return "（未记录）";
  }
  return rev.length > 12 ? `${rev.slice(0, 12)}…` : rev;
}

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
            toast.show(`已保存，但索引刷新失败：${outcome.indexError}`, "danger");
          } else {
            toast.show("已保存（旧版已自动备份为 SKILL.md.bak）", "success");
          }
        },
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  const readOnly = isReadOnlySkill(skill.canonicalPath, tools.data ?? []);
  const toolById = new Map<string, ToolDto>((tools.data ?? []).map((t) => [t.id, t]));
  const myTargets = (targets.data ?? []).filter((t) => t.skillId === skill.id);
  const deployedToolIds = new Set(myTargets.map((t) => t.toolId));
  const deployableTools = (tools.data ?? []).filter((t) => t.enabled && t.installed);

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
            toast.show("内容与磁盘一致，未做改动", "success");
          } else if ("applied" in outcome) {
            toast.show(
              `已更新到 ${shortRevision(outcome.applied.revision)}（旧版已移入回收站）`,
              "success",
            );
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
        onSuccess: () => toast.show(`已部署到 ${toolById.get(toolId)?.displayName ?? toolId}`, "success"),
        onError: (e) => toast.show(ipcError(e), "danger"),
      },
    );
  }

  function revokeFrom(toolId: string) {
    deployRevoke.mutate(
      { skillId: skill.id, tools: [toolId], dryRun: false },
      {
        onSuccess: () => toast.show(`已取消部署到 ${toolById.get(toolId)?.displayName ?? toolId}`, "success"),
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

  // ------------------------------------------------------------------
  // 渲染
  // ------------------------------------------------------------------
  /// §9.3 底部问题列表：错误（阻断保存）在前、告警（不阻断）在后；
  /// 空态文案区分"校验中 / 通过 / 无问题"。
  function renderDocIssueList() {
    if (validating && !report) {
      return <p className="text-xs text-ink-faint">正在校验…</p>;
    }
    const errors = report?.errors ?? [];
    const warnings = report?.warnings ?? [];
    if (errors.length === 0 && warnings.length === 0) {
      return <p className="text-xs text-success">校验通过：frontmatter 完整，未发现其他问题。</p>;
    }
    return (
      <div className="space-y-1.5">
        {errors.map((issue, i) => (
          <DocIssueRow key={`e${i}`} issue={issue} tone="danger" />
        ))}
        {warnings.map((issue, i) => (
          <DocIssueRow key={`w${i}`} issue={issue} tone="blue" />
        ))}
      </div>
    );
  }

  /// 安全体检（§9.1）：打开详情页即扫描（纯静态，只提示不阻断安装）。
  function renderSecurityCard() {
    return (
      <section className="sb-card p-4">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold text-ink">安全体检</h3>
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
            <p className="text-xs leading-5 text-danger">扫描失败：{ipcError(security.error)}</p>
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
        <h3 className="text-sm font-semibold text-ink">更新</h3>
        {skill.originKind === "archive" ? (
          <p className="mt-2 text-xs leading-5 text-ink-secondary">
            压缩包导入的 Skill 不支持在线更新；如需更新，请删除后重新导入新版档案。
          </p>
        ) : (
          <>
            <div className="mt-2 flex items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                disabled={check.isPending}
                onClick={runCheck}
              >
                <RefreshCw size={13} className={check.isPending ? "animate-spin" : ""} />
                {skill.originKind === "git" ? "检查更新" : "检查本地变更"}
              </Button>
            </div>
            {check.isPending && <Skeleton className="mt-3 h-12 w-full rounded-[8px]" />}
            {checkResult && (
              <div className="mt-3 space-y-2 border-t border-line pt-3">
                {checkResult.status === "upToDate" && (
                  <p className="text-xs leading-5 text-success">已是最新{checkResult.note ? `（${checkResult.note}）` : ""}</p>
                )}
                {checkResult.status === "unsupported" && (
                  <p className="text-xs leading-5 text-ink-secondary">{checkResult.note ?? "该来源不支持更新。"}</p>
                )}
                {checkResult.status === "updateAvailable" && (
                  <>
                    <p className="text-xs leading-5 text-ink">
                      {skill.originKind === "git" ? "远端有新版本：" : "磁盘内容有变化："}
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
                      应用更新
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
          <h3 className="text-sm font-semibold text-ink">部署状态</h3>
          <div className="mt-2 flex items-center gap-2">
            <Badge tone="danger" dot>只读来源</Badge>
            <p className="text-xs leading-5 text-ink-secondary">来自插件缓存（extra_scan_dirs），不允许部署。</p>
          </div>
        </section>
      );
    }
    return (
      <section className="sb-card p-4">
        <h3 className="text-sm font-semibold text-ink">部署状态</h3>
        {myTargets.length > 0 ? (
          <div className="mt-2 space-y-2">
            {myTargets.map((t) => (
              <div key={t.toolId} className="flex items-center gap-2 rounded-[8px] bg-surface px-2.5 py-2">
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5 text-xs font-medium text-ink">
                    {toolById.get(t.toolId)?.displayName ?? t.toolId}
                    <Badge tone="blue">{MODE_LABEL[t.mode]}</Badge>
                  </span>
                  <span className="mt-0.5 block truncate font-mono text-xs text-ink-faint" title={t.path}>
                    {t.path}
                  </span>
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={deployRevoke.isPending}
                  onClick={() => revokeFrom(t.toolId)}
                >
                  <Unlink size={13} />
                  取消
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <p className="mt-2 text-xs text-ink-faint">尚未部署到任何工具。</p>
        )}
        {deployableTools.filter((t) => !deployedToolIds.has(t.id)).length > 0 && (
          <div className="mt-3 flex flex-wrap items-center gap-1.5 border-t border-line pt-3">
            <span className="text-xs text-ink-faint">部署到：</span>
            {deployableTools
              .filter((t) => !deployedToolIds.has(t.id))
              .map((t) => (
                <Button key={t.id} size="sm" disabled={deployApply.isPending} onClick={() => deployTo(t.id)}>
                  {(t.icon || t.displayName).slice(0, 2)} {t.displayName}
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
          返回
        </Button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h2 className="truncate text-base font-semibold text-ink">{skill.name}</h2>
            <Badge tone={skill.originKind === "local" ? "neutral" : "blue"}>{ORIGIN_LABEL[skill.originKind]}</Badge>
            <RiskBadge level={security.data?.riskLevel} scanned={!!security.data} />
            {readOnly && <Badge tone="danger" dot>只读来源</Badge>}
          </div>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-ink-secondary">
            {skill.description || "（无描述）"}
          </p>
        </div>
      </div>

      <div className="mt-4 grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_360px] gap-3 overflow-hidden">
        {/* 左列：SKILL.md 预览 / 编辑（§9.3） */}
        <section className="sb-card flex min-h-0 flex-col overflow-hidden p-0">
          <div className="flex items-center gap-2 border-b border-line px-4 py-2.5">
            <h3 className="flex-1 text-sm font-semibold text-ink">
              SKILL.md {editing ? "编辑" : "预览"}
            </h3>
            {editing ? (
              <>
                <span className="text-xs text-ink-faint">
                  {validating ? "校验中…" : hasErrors ? "存在错误，保存被阻止" : "错误会阻止保存；告警不会"}
                </span>
                <Button size="sm" disabled={docSave.isPending} onClick={() => requestDiscard(exitEdit)}>
                  <X size={13} />
                  取消
                </Button>
                <Button
                  variant="primary"
                  size="sm"
                  disabled={docSave.isPending || hasErrors}
                  onClick={saveDoc}
                >
                  <Save size={13} />
                  保存
                </Button>
              </>
            ) : (
              <Button variant="secondary" size="sm" disabled={doc.isLoading || doc.isError} onClick={startEdit}>
                <Pencil size={13} />
                编辑
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
                {renderDocIssueList()}
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
                  title="读取 SKILL.md 失败"
                  description={ipcError(doc.error)}
                  action={
                    <Button variant="primary" size="sm" onClick={() => void qc.invalidateQueries({ queryKey: queryKeys.skillDoc(skill.id) })}>
                      重试
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
        <div className="flex min-h-0 flex-col gap-3 overflow-y-auto pr-0.5">
          <section className="sb-card p-4">
            <h3 className="text-sm font-semibold text-ink">来源溯源</h3>
            <dl className="mt-2 space-y-2 text-xs leading-5">
              <div className="flex gap-2">
                <dt className="w-14 shrink-0 text-ink-faint">类型</dt>
                <dd className="min-w-0 text-ink">{ORIGIN_LABEL[skill.originKind]}</dd>
              </div>
              <div className="flex gap-2">
                <dt className="w-14 shrink-0 text-ink-faint">{skill.originKind === "git" ? "仓库" : skill.originKind === "archive" ? "档案" : "路径"}</dt>
                <dd className="min-w-0 flex-1">
                  <span className="block truncate font-mono text-ink" title={skill.originRef ?? skill.canonicalPath}>
                    {skill.originRef ?? skill.canonicalPath}
                  </span>
                </dd>
              </div>
              {skill.originKind === "git" && skill.originSubpath && (
                <div className="flex gap-2">
                  <dt className="w-14 shrink-0 text-ink-faint">子路径</dt>
                  <dd className="min-w-0 truncate font-mono text-ink" title={skill.originSubpath}>
                    {skill.originSubpath}
                  </dd>
                </div>
              )}
              {skill.originKind === "git" && (
                <div className="flex gap-2">
                  <dt className="w-14 shrink-0 text-ink-faint">版本</dt>
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
                  打开仓库
                </Button>
              )}
              <Button variant="secondary" size="sm" disabled={openPath.isPending} onClick={openCanonicalDir}>
                <FolderOpen size={13} />
                打开目录
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
        title="应用更新"
        footer={
          <>
            <Button onClick={() => setShowApplyConfirm(false)}>再想想</Button>
            <Button variant="primary" disabled={apply.isPending} onClick={runApply}>
              确认更新
            </Button>
          </>
        }
      >
        <p>
          将用新内容替换「{skill.name}」的正本；旧版本会移入回收站，不会直接抹除。
          {checkResult?.diffSummary ? " 上方已展示更新前 diff 摘要。" : ""}
        </p>
      </Modal>

      {/* 丢弃未保存草稿确认（取消编辑 / 返回列表） */}
      <Modal
        open={discardAction !== null}
        onClose={() => setDiscardAction(null)}
        title="放弃未保存的修改？"
        footer={
          <>
            <Button onClick={() => setDiscardAction(null)}>继续编辑</Button>
            <Button
              variant="danger"
              onClick={() => {
                discardAction?.();
                setDiscardAction(null);
              }}
            >
              放弃修改
            </Button>
          </>
        }
      >
        <p>编辑器中还有未保存的改动，放弃后将无法找回（保存前的自动备份只在保存时生成）。</p>
      </Modal>
    </div>
  );
}

/// 底部问题列表单行：行号 + 规则人话标签 + 后端原文说明。
function DocIssueRow({ issue, tone }: { issue: DocIssue; tone: "danger" | "blue" }) {
  return (
    <div className="flex items-start gap-2 rounded-[8px] bg-surface px-2.5 py-1.5">
      <Badge tone={tone} dot>
        {DOC_RULE_LABEL[issue.rule] ?? issue.rule}
      </Badge>
      <p className="min-w-0 flex-1 text-xs leading-5 text-ink-secondary">
        {issue.line > 0 && <span className="mr-1.5 font-mono text-ink-faint">第 {issue.line} 行</span>}
        {issue.message}
      </p>
    </div>
  );
}
