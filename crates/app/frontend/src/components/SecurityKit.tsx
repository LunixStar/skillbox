/// 安全扫描共享 UI（§9.1）：规则人话标签、风险徽章、发现列表。
/// 库卡片 / 列表行、详情页、工具箱共用，避免三处各自维护文案。

import type { RiskLevel, SecurityFinding } from "../api/ipc";
import { Badge } from "./ui";

/// 八条规则的人话标签（§11.4 人话原则）。
export const RULE_LABEL: Record<string, string> = {
  shell_exec: "执行 shell 命令",
  network_egress: "网络访问",
  credential_access: "读取凭据",
  path_traversal: "路径越界",
  obfuscation: "混淆代码",
  symlink_escape: "符号链接越界",
  hidden_files: "隐藏文件",
  excessive_scope: "过宽的权限声明",
};

export function ruleLabel(ruleId: string): string {
  return RULE_LABEL[ruleId] ?? ruleId;
}

export function riskLabel(level: RiskLevel): string {
  return level === "high" ? "高风险" : level === "medium" ? "中风险" : "低风险";
}

type BadgeTone = "blue" | "neutral" | "success" | "danger";

function riskTone(level: RiskLevel): BadgeTone {
  return level === "high" ? "danger" : level === "medium" ? "blue" : "success";
}

function severityTone(severity: SecurityFinding["severity"]): BadgeTone {
  return severity === "critical" ? "danger" : severity === "warning" ? "blue" : "neutral";
}

const SEVERITY_LABEL: Record<SecurityFinding["severity"], string> = {
  info: "提示",
  warning: "警告",
  critical: "严重",
};

/// 风险徽章：未扫描 → 「未体检」；已扫描 → 低/中/高风险。
export function RiskBadge({ level, scanned }: { level?: RiskLevel; scanned: boolean }) {
  if (!scanned || !level) {
    return <Badge tone="neutral">未体检</Badge>;
  }
  return (
    <Badge tone={riskTone(level)} dot={level !== "low"}>
      {riskLabel(level)}
    </Badge>
  );
}

/// 逐条发现列表（详情页 / 工具箱共用）：规则 · 严重度 · 位置 · 片段 · 建议。
export function FindingList({ findings }: { findings: SecurityFinding[] }) {
  if (findings.length === 0) {
    return <p className="text-xs leading-5 text-success">未发现风险点（纯静态扫描，仅供参考）。</p>;
  }
  return (
    <ul className="space-y-1.5">
      {findings.map((f, i) => (
        <li key={`${f.ruleId}-${f.file}-${f.line}-${i}`} className="rounded-[8px] bg-surface px-2.5 py-2">
          <div className="flex items-center gap-1.5">
            <Badge tone={severityTone(f.severity)}>{SEVERITY_LABEL[f.severity]}</Badge>
            <span className="text-xs font-medium text-ink">{ruleLabel(f.ruleId)}</span>
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
