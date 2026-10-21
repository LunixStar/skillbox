/// 类型化 IPC 层：Rust 命令（camelCase 自动映射）的 TS 镜像 + invoke 封装。
/// 所有字段的来源是各 crate 的 serde 结构体（见 ipc/src/*.rs）。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// 类型镜像（Rust → TS，serde rename_all = "camelCase"）
// ---------------------------------------------------------------------------

export interface SkillMeta {
  id: number;
  name: string;
  description: string;
  originKind: "local" | "git" | "archive";
  originRef: string | null;
  originSubpath: string | null;
  originRevision: string | null;
  canonicalPath: string;
  contentHash: string;
  createdAt: number;
  updatedAt: number;
}

export interface TargetRow {
  skillId: number;
  toolId: string;
  path: string;
  mode: "symlink" | "copy";
  syncedAt: number;
  sourceHash: string | null;
}

export interface ToolDto {
  id: string;
  displayName: string;
  formFactor: "cli" | "ide" | "editorPlugin" | "terminalTui" | "desktopAssistant";
  installed: boolean;
  deployDir: string | null;
  linkCapability: "symlink" | "junction" | "copyOnly";
  icon: string;
  docsUrl: string;
  enabled: boolean;
  /// 插件缓存目录（extra_scan_dirs，已解析）：只读来源（§4.3）。
  extraScanDirs: string[];
}

export interface ScanProgress {
  scanned: number;
  totalHint: number | null;
  current: string;
  found: number;
  currentTool: string | null;
  foundByTool: Record<string, number>;
}

export interface DiscoveredSkill {
  toolId: string | null;
  path: string;
  dirName: string;
  name: string | null;
  description: string | null;
  fingerprint: { mtimeMs: number; size: number; childCount: number };
  readOnly: boolean;
}

export interface ScanReport {
  skills: DiscoveredSkill[];
  skipped: { path: string; reason: "blacklisted" | "tooDeep" | "symlink" | "nonLocalVolume" }[];
  warnings: string[];
  durationMs: number;
}

export interface ScanStatusInfo {
  running: boolean;
  startedAtMs: number | null;
  report: ScanReport | null;
  error: string | null;
}

export interface InstallOutcome {
  skillId: number | null;
  name: string;
  canonicalPath: string;
  dryRun: boolean;
}

// ---------------------------------------------------------------------------
// 更新（updates_check / updates_apply；§9.4 溯源·更新）
// ---------------------------------------------------------------------------

export type UpdateStatus = "upToDate" | "updateAvailable" | "unsupported";

export interface UpdateCheckOutcome {
  status: UpdateStatus;
  currentRevision: string | null;
  newRevision: string | null;
  diffSummary: string | null;
  note: string | null;
}

export interface UpdateReport {
  revision: string | null;
  diffSummary: string | null;
}

/// serde 外部标签：`{ "applied": {...} }` / `"upToDate"` / `{ "preview": {...} }` /
/// `{ "unsupported": { "reason": "..." } }`。
export type UpdateOutcome =
  | { applied: UpdateReport }
  | "upToDate"
  | { preview: UpdateReport }
  | { unsupported: { reason: string } };

// ---------------------------------------------------------------------------
// invoke 封装（仅本步骤 UI 用到的命令；全集见 dev/IPCTester）
// ---------------------------------------------------------------------------

export const scanStart = () => invoke<void>("scan_start");
export const scanCancel = () => invoke<void>("scan_cancel");
export const scanStatus = () => invoke<ScanStatusInfo>("scan_status");

export const libraryList = () => invoke<SkillMeta[]>("library_list");
export const libraryTargets = () => invoke<TargetRow[]>("library_targets");

export function libraryInstallLocal(opts: {
  path: string;
  name: string | null;
  keepInPlace: boolean;
  dryRun: boolean;
}) {
  return invoke<InstallOutcome>("library_install_local", opts);
}

export function libraryRemove(opts: {
  skillId: number;
  mode: "undeploy" | "delete";
  dryRun: boolean;
}) {
  return invoke<unknown>("library_remove", opts);
}

export function deployApply(opts: {
  skillId: number;
  tools: string[];
  mode: "symlink" | "copy";
  overwrite: boolean;
  dryRun: boolean;
}) {
  return invoke<unknown>("deploy_apply", opts);
}

export function deployRevoke(opts: {
  skillId: number;
  tools: string[];
  dryRun: boolean;
}) {
  return invoke<unknown>("deploy_revoke", opts);
}

export const toolsList = () => invoke<ToolDto[]>("tools_list");

export function toolsSetEnabled(opts: { toolId: string; enabled: boolean; dryRun: boolean }) {
  return invoke<unknown>("tools_set_enabled", opts);
}

export function updatesCheck(skillId: number) {
  return invoke<UpdateCheckOutcome>("updates_check", { skillId });
}

export function updatesApply(skillId: number, dryRun: boolean) {
  return invoke<UpdateOutcome>("updates_apply", { skillId, dryRun });
}

export function libraryGet(skillId: number) {
  return invoke<SkillMeta>("library_get", { skillId });
}

export function libraryDocRead(skillId: number) {
  return invoke<string>("library_doc_read", { skillId });
}

export function libraryDocWrite(opts: { skillId: number; contents: string; dryRun: boolean }) {
  return invoke<unknown>("library_doc_write", opts);
}

export function settingsGet(key: string) {
  return invoke<string | null>("settings_get", { key });
}

export function settingsSet(opts: { key: string; value: string; dryRun: boolean }) {
  return invoke<unknown>("settings_set", opts);
}

export function systemOpenPath(id: string) {
  return invoke<unknown>("system_open_path", { id });
}

/// §4.3 只读来源判定（前端侧）：正本位于任一工具的插件缓存
/// （extra_scan_dirs）内 → 只读，不可部署（后端另有硬守卫）。
export function isReadOnlySkill(canonicalPath: string, tools: ToolDto[]): boolean {
  return tools.some((t) =>
    t.extraScanDirs.some(
      (d) => canonicalPath === d || canonicalPath.startsWith(d + "\\") || canonicalPath.startsWith(d + "/"),
    ),
  );
}

// ---------------------------------------------------------------------------
// 错误规范化：invoke reject 的值可能是 AppError 序列化对象、字符串或 Error。
// ---------------------------------------------------------------------------

export function ipcError(e: unknown): string {
  if (typeof e === "string") {
    return e;
  }
  if (e && typeof e === "object") {
    const m = (e as { message?: unknown }).message;
    if (typeof m === "string" && m.length > 0) {
      return m;
    }
  }
  return String(e);
}

// ---------------------------------------------------------------------------
// 扫描事件（Tauri event，scan.rs 工作线程推送）
// ---------------------------------------------------------------------------

export interface ScanEventHandlers {
  onProgress?: (p: ScanProgress) => void;
  onDone?: (report: ScanReport) => void;
  onError?: (message: string) => void;
}

/// 订阅三类扫描事件；返回解除函数（组件卸载时调用）。
export async function listenScanEvents(handlers: ScanEventHandlers): Promise<UnlistenFn> {
  const offs = await Promise.all([
    listen<ScanProgress>("scan://progress", (e) => handlers.onProgress?.(e.payload)),
    listen<ScanReport>("scan://done", (e) => handlers.onDone?.(e.payload)),
    // AppError 序列化为结构体；这里只取 message 字段（缺失时兜底整串）。
    listen<Record<string, unknown>>("scan://error", (e) => {
      const p = e.payload;
      const msg = typeof p?.message === "string" ? p.message : JSON.stringify(p);
      handlers.onError?.(msg);
    }),
  ]);
  return () => offs.forEach((off) => off());
}
