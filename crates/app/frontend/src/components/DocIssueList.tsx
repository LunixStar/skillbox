/// SKILL.md 编辑器底部问题列表（§9.3）：错误（阻断保存）在前、
/// 告警（不阻断）在后；空态文案区分「校验中 / 通过」。
/// 从 SkillDetailPage 拆出以守住 600 行上限。

import type { DocCheckReport, DocIssue } from "../api/ipc";
import { useI18n } from "../i18n";
import { Badge } from "./ui";

/// §9.3 校验规则人话标签 key（未知规则回退 rule id 本身）。
const DOC_RULE_LABEL_KEY: Record<string, string> = {
  missing_name: "detail.docRule.missing_name",
  invalid_name: "detail.docRule.invalid_name",
  name_mismatch: "detail.docRule.name_mismatch",
  missing_description: "detail.docRule.missing_description",
  short_description: "detail.docRule.short_description",
  long_description: "detail.docRule.long_description",
  unclosed_fence: "detail.docRule.unclosed_fence",
  missing_file: "detail.docRule.missing_file",
};

export function DocIssueList({ validating, report }: { validating: boolean; report: DocCheckReport | null }) {
  const { t } = useI18n();
  if (validating && !report) {
    return <p className="text-xs text-ink-faint">{t("detail.validating")}</p>;
  }
  const errors = report?.errors ?? [];
  const warnings = report?.warnings ?? [];
  if (errors.length === 0 && warnings.length === 0) {
    return <p className="text-xs text-success">{t("detail.validationOk")}</p>;
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

/// 单行：行号 + 规则人话标签 + 后端原文说明。
function DocIssueRow({ issue, tone }: { issue: DocIssue; tone: "danger" | "blue" }) {
  const { t } = useI18n();
  return (
    <div className="flex items-start gap-2 rounded-[8px] bg-surface px-2.5 py-1.5">
      <Badge tone={tone} dot>
        {t(DOC_RULE_LABEL_KEY[issue.rule] ?? issue.rule)}
      </Badge>
      <p className="min-w-0 flex-1 text-xs leading-5 text-ink-secondary">
        {issue.line > 0 && <span className="mr-1.5 font-mono text-ink-faint">{t("detail.issueLine", { n: issue.line })}</span>}
        {issue.message}
      </p>
    </div>
  );
}
