/// IPC 命令测试页（开发用，§8 DoD：31 个命令全部可点击）。
/// 每个命令一行：可编辑参数（JSON，camelCase 键）→ Run → 结果面板。
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Cmd = { name: string; args?: Record<string, unknown> };
type Group = { title: string; cmds: Cmd[] };

const GROUPS: Group[] = [
  {
    title: "scan (3)",
    cmds: [
      { name: "scan_start" },
      { name: "scan_cancel" },
      { name: "scan_status" },
    ],
  },
  {
    title: "library (9)",
    cmds: [
      { name: "library_list" },
      { name: "library_get", args: { skillId: 1 } },
      {
        name: "library_install_local",
        args: { path: "C:\\tmp\\demo-skill", name: null, keepInPlace: false, dryRun: true },
      },
      {
        name: "library_install_git",
        args: { url: "https://github.com/user/skill", subpath: null, revision: null, name: null, dryRun: true },
      },
      { name: "library_remove", args: { skillId: 1, mode: "undeploy", dryRun: true } },
      { name: "library_doc_read", args: { skillId: 1 } },
      {
        name: "library_doc_write",
        args: { skillId: 1, contents: "---\nname: demo\ndescription: demo\n---\nbody", dryRun: true },
      },
      { name: "library_doc_validate", args: { skillId: 1 } },
    ],
  },
  {
    title: "deploy (3)",
    cmds: [
      { name: "deploy_apply", args: { skillId: 1, tools: ["claude_code"], mode: "symlink", overwrite: false, dryRun: true } },
      { name: "deploy_revoke", args: { skillId: 1, tools: [], dryRun: true } },
      { name: "deploy_set_mode", args: { skillId: 1, tool: "claude_code", mode: "copy", dryRun: true } },
    ],
  },
  {
    title: "tools (2)",
    cmds: [
      { name: "tools_list" },
      { name: "tools_set_enabled", args: { toolId: "claude_code", enabled: true, dryRun: true } },
    ],
  },
  {
    title: "doctor (2)",
    cmds: [
      { name: "doctor_run", args: { tools: null, minSeverity: null } },
      { name: "doctor_fix", args: { items: [], confirmed: false, dryRun: true } },
    ],
  },
  {
    title: "panels (3)",
    cmds: [
      { name: "security_scan", args: { skillId: 1 } },
      { name: "security_report" },
      { name: "cost_report" },
    ],
  },
  {
    title: "system (6)",
    cmds: [
      { name: "system_open_path", args: { id: "central" } },
      { name: "system_logs_export" },
      { name: "system_cache_inspect" },
      { name: "system_cache_clear", args: { names: null, dryRun: true } },
      { name: "system_trash_list" },
      { name: "system_trash_restore", args: { slot: "0", name: "demo", dest: "central", dryRun: true } },
    ],
  },
  {
    title: "settings (2)",
    cmds: [
      { name: "settings_get", args: { key: "tools.disabled" } },
      { name: "settings_set", args: { key: "demo", value: "1", dryRun: true } },
    ],
  },
  {
    title: "updates (2)",
    cmds: [
      { name: "updates_check", args: { skillId: 1 } },
      { name: "updates_apply", args: { skillId: 1, dryRun: true } },
    ],
  },
];

function CmdRow({ cmd }: { cmd: Cmd }) {
  const [text, setText] = useState(JSON.stringify(cmd.args ?? {}));
  const [out, setOut] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function run() {
    setBusy(true);
    setOut(null);
    try {
      const args: Record<string, unknown> = text.trim() ? JSON.parse(text) : {};
      const result = await invoke(cmd.name, args);
      setOut(JSON.stringify(result, null, 2));
    } catch (e) {
      setOut(typeof e === "string" ? e : JSON.stringify(e, null, 2));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="border-b border-line px-3 py-2 last:border-b-0">
      <div className="flex items-center gap-2">
        <button
          className="rounded bg-primary px-2.5 py-1 text-xs font-medium text-white disabled:opacity-50"
          onClick={run}
          disabled={busy}
        >
          Run
        </button>
        <span className="font-mono text-xs text-ink">{cmd.name}</span>
      </div>
      <textarea
        className="mt-1.5 h-9 w-full resize-y rounded border border-line bg-white px-2 py-1 font-mono text-[11px] text-ink-secondary"
        value={text}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
      />
      {out !== null && (
        <pre className="mt-1.5 max-h-48 overflow-auto rounded bg-ink/95 px-2 py-1.5 font-mono text-[11px] leading-relaxed text-white">
          {out}
        </pre>
      )}
    </div>
  );
}

export default function IPCTester() {
  return (
    <div className="flex h-screen flex-col bg-surface">
      <div className="h-[2px] w-full bg-gradient-to-r from-primary via-[#56C8F0] to-primary" />
      <div className="flex-1 overflow-auto p-4">
        <h1 className="mb-3 text-base font-semibold text-ink">IPC Tester（31 命令）</h1>
        <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
          {GROUPS.map((g) => (
            <section key={g.title} className="rounded-[10px] border border-line bg-white shadow-[0_1px_3px_rgba(16,40,80,0.06)]">
              <h2 className="border-b border-line px-3 py-2 text-xs font-semibold text-ink-secondary">{g.title}</h2>
              {g.cmds.map((c) => (
                <CmdRow key={c.name} cmd={c} />
              ))}
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
