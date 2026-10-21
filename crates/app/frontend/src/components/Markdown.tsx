/// 自建轻量 Markdown 渲染器（约 140 行）：SKILL.md 预览用。
/// 不引入 markdown 渲染库（依赖白名单外）；全部输出 React 元素，
/// 无 dangerouslySetInnerHTML（转义安全）。
/// 支持：frontmatter 剥离、标题/引用/有序无序列表/代码块/段落、
/// 行内 **粗体** / *斜体* / `code` / [text](url)。

import { type ReactNode } from "react";

// ---------------------------------------------------------------------------
// 行内解析：**bold** / *italic* / `code` / [text](url)
// ---------------------------------------------------------------------------

const INLINE_RE = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\([^)\s]+\))/g;

function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const parts = text.split(INLINE_RE).filter((p) => p !== "" && p !== undefined);
  return parts.map((part, i) => {
    const key = `${keyPrefix}-${i}`;
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return (
        <strong key={key} className="font-semibold text-ink">
          {part.slice(2, -2)}
        </strong>
      );
    }
    if (part.startsWith("*") && part.endsWith("*") && part.length > 2) {
      return (
        <em key={key} className="italic">
          {part.slice(1, -1)}
        </em>
      );
    }
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return (
        <code
          key={key}
          className="rounded-[4px] bg-surface px-1 py-0.5 font-mono text-[0.85em] text-primary-active"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(part);
    if (link) {
      return (
        <a
          key={key}
          href={link[2]}
          target="_blank"
          rel="noreferrer"
          className="text-primary hover:text-primary-hover underline"
        >
          {link[1]}
        </a>
      );
    }
    return <span key={key}>{part}</span>;
  });
}

// ---------------------------------------------------------------------------
// 块级解析：逐行状态机
// ---------------------------------------------------------------------------

interface MarkdownProps {
  source: string;
  className?: string;
}

export function Markdown({ source, className }: MarkdownProps) {
  const body = stripFrontmatter(source);
  const lines = body.split(/\r?\n/);

  const blocks: ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === "") {
      i += 1;
      continue;
    }

    // 代码块：``` 围栏
    if (line.trimStart().startsWith("```")) {
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        code.push(lines[i]);
        i += 1;
      }
      i += 1; // 跳过闭合围栏（文件末尾缺失时也安全终止）
      blocks.push(
        <pre
          key={key++}
          className="overflow-x-auto rounded-card border border-line bg-surface p-3 font-mono text-xs leading-5 text-ink"
        >
          <code>{code.join("\n")}</code>
        </pre>,
      );
      continue;
    }

    // 标题：#{1..6}
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      const cls: Record<number, string> = {
        1: "text-lg font-semibold",
        2: "text-base font-semibold",
        3: "text-sm font-semibold",
        4: "text-sm font-semibold text-ink-secondary",
        5: "text-xs font-semibold text-ink-secondary",
        6: "text-xs font-semibold text-ink-faint",
      };
      blocks.push(
        <div key={key++} className={`${cls[level]} text-ink`}>
          {renderInline(heading[2], `h${key}`)}
        </div>,
      );
      i += 1;
      continue;
    }

    // 引用块：连续 > 行
    if (line.startsWith(">")) {
      const quote: string[] = [];
      while (i < lines.length && lines[i].startsWith(">")) {
        quote.push(lines[i].replace(/^>\s?/, ""));
        i += 1;
      }
      blocks.push(
        <blockquote
          key={key++}
          className="border-l-2 border-[rgba(59,142,240,0.5)] pl-3 text-xs text-ink-secondary"
        >
          {renderInline(quote.join(" "), `q${key}`)}
        </blockquote>,
      );
      continue;
    }

    // 无序列表：连续 - / * / + 行
    if (/^\s*[-*+]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*[-*+]\s+/, ""));
        i += 1;
      }
      blocks.push(
        <ul key={key++} className="list-disc space-y-1 pl-5 text-sm leading-6 text-ink">
          {items.map((item, j) => (
            <li key={j}>{renderInline(item, `ul${key}-${j}`)}</li>
          ))}
        </ul>,
      );
      continue;
    }

    // 有序列表：连续 1. / 2. 行
    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*\d+\.\s+/, ""));
        i += 1;
      }
      blocks.push(
        <ol key={key++} className="list-decimal space-y-1 pl-5 text-sm leading-6 text-ink">
          {items.map((item, j) => (
            <li key={j}>{renderInline(item, `ol${key}-${j}`)}</li>
          ))}
        </ol>,
      );
      continue;
    }

    // 段落：连续非空非格式行合并
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !lines[i].trimStart().startsWith("```") &&
      !/^(#{1,6})\s+/.test(lines[i]) &&
      !lines[i].startsWith(">") &&
      !/^\s*([-*+]|\d+\.)\s+/.test(lines[i])
    ) {
      para.push(lines[i]);
      i += 1;
    }
    blocks.push(
      <p key={key++} className="text-sm leading-6 text-ink">
        {renderInline(para.join(" "), `p${key}`)}
      </p>,
    );
  }

  return <div className={`flex flex-col gap-3 ${className ?? ""}`}>{blocks}</div>;
}

/// 剥离 frontmatter（--- ... --- 开头块）。
function stripFrontmatter(source: string): string {
  if (!source.startsWith("---")) {
    return source;
  }
  const end = source.indexOf("\n---", 3);
  if (end === -1) {
    return source;
  }
  const after = source.indexOf("\n", end + 1);
  return after === -1 ? "" : source.slice(after + 1);
}
