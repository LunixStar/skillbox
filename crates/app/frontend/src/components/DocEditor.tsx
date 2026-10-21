/// SKILL.md 编辑器（§9.3）：CodeMirror 6（禁 Monaco）。
/// - Markdown 高亮（@codemirror/lang-markdown），```yaml 围栏用真 yaml 解析高亮；
/// - YAML frontmatter 装饰高亮（首段 `---` 区域：键着色 + 分隔符淡显 + 区域底色）；
/// - 校验结果行内标记（错误行红 / 告警行蓝，来源 library_doc_validate 的行号）。

import { useEffect, useMemo, useRef } from "react";
import CodeMirror, {
  Decoration,
  type DecorationSet,
  EditorView,
  type Extension,
  type Range,
  ReactCodeMirrorRef,
  StateEffect,
  StateField,
  ViewPlugin,
  type ViewUpdate,
} from "@uiw/react-codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { yaml } from "@codemirror/lang-yaml";
import type { DocIssue } from "../api/ipc";

/// ```yaml / ```yml 围栏内的真 YAML 解析高亮。
const yamlSupport = yaml();

const mdLanguage = markdown({
  base: markdownLanguage,
  codeLanguages: (info) => (info === "yaml" || info === "yml" ? yamlSupport.language : null),
});

// ---------------------------------------------------------------------------
// YAML frontmatter 装饰高亮
// ---------------------------------------------------------------------------

const fmSep = Decoration.mark({ class: "sb-fm-sep" });
const fmKey = Decoration.mark({ class: "sb-fm-key" });
const fmLine = Decoration.line({ class: "sb-fm-line" });

/// 首段 `---` … `---` 之间：整行底色 + `key:` 键着色 + `---` 淡显。
/// 与后端 frontmatter 判定同规则（首行必须是 `---`）。
function buildFrontmatter(view: EditorView): DecorationSet {
  const doc = view.state.doc;
  const ranges: Range<Decoration>[] = [];
  if (doc.lines < 1 || doc.line(1).text.trimEnd() !== "---") {
    return Decoration.none;
  }
  const first = doc.line(1);
  ranges.push(fmSep.range(0, 3));
  ranges.push(fmLine.range(first.from));
  for (let n = 2; n <= doc.lines; n += 1) {
    const line = doc.line(n);
    ranges.push(fmLine.range(line.from));
    if (line.text.trimEnd() === "---") {
      ranges.push(fmSep.range(line.from, Math.min(line.from + 3, line.to)));
      break;
    }
    const m = /^(\s*)([A-Za-z0-9_-]+)(:)/.exec(line.text);
    if (m) {
      const keyStart = line.from + m[1].length;
      ranges.push(fmKey.range(keyStart, keyStart + m[2].length));
    }
  }
  return Decoration.set(ranges);
}

const frontmatterPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildFrontmatter(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) {
        this.decorations = buildFrontmatter(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

// ---------------------------------------------------------------------------
// 校验行内标记（错误红 / 告警蓝；行号来自 DocIssue.line）
// ---------------------------------------------------------------------------

interface DiagLine {
  line: number;
  severity: "error" | "warning";
}

const setDiag = StateEffect.define<DiagLine[]>();

const diagField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    value = value.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setDiag)) {
        const doc = tr.state.doc;
        const taken = new Set<number>();
        const ranges: Range<Decoration>[] = [];
        // 错误优先：同一行同时命中错误与告警时只标错误。
        const ordered = [...e.value].sort((a, b) =>
          a.severity === b.severity ? a.line - b.line : a.severity === "error" ? -1 : 1,
        );
        for (const d of ordered) {
          const n = Math.min(Math.max(1, d.line), doc.lines);
          if (d.line <= 0 || taken.has(n)) {
            continue;
          }
          taken.add(n);
          const line = doc.line(n);
          const cls = d.severity === "error" ? "sb-diag-error" : "sb-diag-warn";
          ranges.push(Decoration.line({ class: cls }).range(line.from));
        }
        value = Decoration.set(ranges);
      }
    }
    return value;
  },
  provide: (f) => EditorView.decorations.from(f),
});

// ---------------------------------------------------------------------------
// 主题（设计令牌 CSS 变量）
// ---------------------------------------------------------------------------

const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px", backgroundColor: "transparent" },
  ".cm-content": {
    fontFamily: "ui-monospace, SFMono-Regular, Consolas, 'Courier New', monospace",
    caretColor: "var(--sb-primary)",
  },
  ".cm-gutters": { backgroundColor: "transparent", color: "var(--sb-ink-faint)", border: "none" },
  ".cm-activeLine": { backgroundColor: "rgba(59, 142, 240, 0.05)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent" },
  "&.cm-focused": { outline: "none" },
});

// ---------------------------------------------------------------------------
// 组件
// ---------------------------------------------------------------------------

export interface DocEditorProps {
  value: string;
  onChange: (value: string) => void;
  /// 校验发现（错误 + 告警），行内标记数据源。
  errors: DocIssue[];
  warnings: DocIssue[];
}

export default function DocEditor({ value, onChange, errors, warnings }: DocEditorProps) {
  const ref = useRef<ReactCodeMirrorRef>(null);

  const extensions = useMemo<Extension[]>(
    () => [mdLanguage, frontmatterPlugin, diagField, editorTheme],
    [],
  );

  /// 校验结果变化 → dispatch 行内标记（error 优先已在 diagField 内处理）。
  const diagKey = useMemo(
    () =>
      [
        ...errors.map((i) => `e${i.line}:${i.rule}`),
        ...warnings.map((i) => `w${i.line}:${i.rule}`),
      ].join(","),
    [errors, warnings],
  );

  useEffect(() => {
    const view = ref.current?.view;
    if (!view) {
      return;
    }
    const diags: DiagLine[] = [
      ...errors.map((i) => ({ line: i.line, severity: "error" as const })),
      ...warnings.map((i) => ({ line: i.line, severity: "warning" as const })),
    ];
    view.dispatch({ effects: setDiag.of(diags) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [diagKey]);

  return (
    <CodeMirror
      ref={ref}
      value={value}
      onChange={onChange}
      extensions={extensions}
      height="100%"
      style={{ height: "100%" }}
      basicSetup={{ foldGutter: false, highlightActiveLine: true }}
    />
  );
}
