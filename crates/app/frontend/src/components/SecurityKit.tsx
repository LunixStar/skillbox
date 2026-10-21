/// 安全扫描共享 UI（§9.1）：规则人话标签、风险徽章、发现列表。
/// 库卡片 / 列表行、详情页、工具箱共用，避免三处各自维护文案。

import type { RiskLevel, SecurityFinding } from "../api/ipc";
import { Badge } from "./ui";
import { useI18n } from "../i18n";

const RULE_LABEL_KEY: Record<string, string> = {
  shell_exec: "security.rule.shell_exec",
  network_egress: "security.rule.network_egress",
  credential_access: "security.rule.credential_access",
  path_traversal: "security.rule.path_traversal",
  obfuscation: "security.rule.obfuscation",
  symlink_escape: "security.rule.symlink_escape",
  hidden_files: "security.rule.hidden_files",
  excessive_scope: "security.rule.excessive_scope",
};

type BadgeTone = "blue" | "neutral" | "success" | "danger";

function riskTone(level: RiskLevel): BadgeTone {
  return level === "high" ? "danger" : level === "medium" ? "blue" : "success";
}

function severityTone(severity: SecurityFinding["severity"]): BadgeTone {
  return severity === "critical" ? "danger" : severity === "warning" ? "blue" : "neutral";
}

const SEVERITY_LABEL_KEY: Record<SecurityFinding["severity"], string> = {
  info: "security.severityInfo",
  warning: "security.severityWarning",
  critical: "security.severityCritical",
};

/// 风险徽章：未扫描 → 「未体检」；已扫描 → 低/中/高风险。
export function RiskBadge({ level, scanned }: { level?: RiskLevel; scanned: boolean }) {
  const { t } = useI18n();
  if (!scanned || !level) {
    return <Badge tone="neutral">{t("security.unscanned")}</Badge>;
  }
  const label =
    level === "high" ? t("security.riskHigh") : level === "medium" ? t("security.riskMedium") : t("security.riskLow");
  return (
    <Badge tone={riskTone(level)} dot={level !== "low"}>
      {label}
    </Badge>
  );
}

/// 逐条发现列表（详情页 / 工具箱共用）：规则 · 严重度 · 位置 · 片段 · 建议。
export function FindingList({ findings }: { findings: SecurityFinding[] }) {
  const { t } = useI18n();
  if (findings.length === 0) {
    return <p className="text-xs leading-5 text-success">{t("security.noFindings")}</p>;
  }
  return (
    <ul className="space-y-1.5">
      {findings.map((f, i) => (
        <li key={`${f.ruleId}-${f.file}-${f.line}-${i}`} className="rounded-[8px] bg-surface px-2.5 py-2">
          <div className="flex items-center gap-1.5">
            <Badge tone={severityTone(f.severity)}>{t(SEVERITY_LABEL_KEY[f.severity])}</Badge>
            <span className="text-xs font-medium text-ink">{t(RULE_LABEL_KEY[f.ruleId] ?? f.ruleId)}</span>
          </div>
          <div className="mt-1 truncate font-mono text-xs text-ink-faint" title={`${f.file}:${f.line}`}>
            {f.file}
            {f.line > 0 ? `:${f.line}` : ""}
          </div>
          {f.snippet && (
            <pre className="mt-1 max-h-20 overflow-y-auto whitespace-pre-wrap break-all rounded-[6px] bg-page px-2 py-1.5 font-mono text-xs leading-5 text-ink-secondary">
              {f.snippet}
            </pre>
          )}
          <p className="mt-1 text-xs leading-5 text-ink-secondary">{f.advice}</p>
        </li>
      ))}
    </ul>
  );
}
