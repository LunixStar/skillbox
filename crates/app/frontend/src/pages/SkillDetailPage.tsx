/// Skill 详情页（§9.4）：SKILL.md 预览 · 来源溯源 · 更新 · 部署状态。
/// 更新语义按来源分派：git=fetch 比对（先看 diff 摘要再应用）、
/// local=重新导入比对、archive=不支持在线更新（诚实提示）。

import { useState } from "react";
import {
  ArrowLeft,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  Unlink,
} from "lucide-react";
import {
  ipcError,
  isReadOnlySkill,
  type SkillMeta,
  type TargetRow,
  type ToolDto,
  type UpdateCheckOutcome,
  type UpdateOutcome,
} from "../api/ipc";
import {
  queryKeys,
  useDeployApply,
  useDeployRevoke,
  useOpenPath,
  useSecurityReport,
  useSkillDoc,
  useTargets,
  useTools,
  useUpdateApply,
  useUpdateCheck,
} from "../api/queries";
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
  const openPath = useOpenPath();
  const toast = useToast();
  const qc = useQueryClient();

  const [checkResult, setCheckResult] = useState<UpdateCheckOutcome | null>(null);
  const [showApplyConfirm, setShowApplyConfirm] = useState(false);

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
        <Button variant="ghost" size="sm" onClick={onBack}>
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
        {/* 左列：SKILL.md 预览 */}
        <section className="sb-card flex min-h-0 flex-col overflow-hidden p-0">
          <h3 className="border-b border-line px-4 py-3 text-sm font-semibold text-ink">SKILL.md 预览</h3>
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
    </div>
  );
}
