/// 命令面板（§11.3）：Ctrl+K 呼出，专业用户的快捷入口。
/// 覆盖全部无复杂参数的 IPC 写操作（扫描 / 部署 / 撤销 / 删除 / 工具开关 /
/// 登记本地目录 / 语言持久化 / 安全体检 / 部署模式切换）+ 视图导航。
/// 例外（需页面上下文，保留在原页面流内）：doc_write（编辑器内容）、
/// doctor_fix（dry-run 影响清单 + 逐项确认）、updates_apply（diff 摘要确认）。

import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { CornerDownLeft, Search } from "lucide-react";
import {
  deploySetMode,
  ipcError,
  isReadOnlySkill,
  scanCancel,
  scanStart,
  securityScan,
  type ToolDto,
} from "../api/ipc";
import {
  queryKeys,
  useDeployApply,
  useDeployRevoke,
  useInstallLocal,
  useLibrary,
  useOpenPath,
  useRemoveSkill,
  useSetToolEnabled,
  useTargets,
  useTools,
} from "../api/queries";
import { useToast } from "./Toast";
import { Button, Modal } from "./ui";
import { useI18n } from "../i18n";
import type { ShellView } from "../shell/Sidebar";

interface Command {
  id: string;
  label: string;
  /// 分组标题（导航 / 操作 / 技能 / 工具）。
  group: string;
  /// 过滤附加关键词（工具名、skill 名等）。
  keywords?: string;
  danger?: boolean;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (view: ShellView) => void;
}

export default function CommandPalette({ open, onClose, onNavigate }: CommandPaletteProps) {
  const library = useLibrary();
  const tools = useTools();
  const targets = useTargets();
  const deployApply = useDeployApply();
  const deployRevoke = useDeployRevoke();
  const removeSkill = useRemoveSkill();
  const install = useInstallLocal();
  const setToolEnabled = useSetToolEnabled();
  const openPath = useOpenPath();
  const toast = useToast();
  const qc = useQueryClient();
  const { t, lang, setLang } = useI18n();

  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  /// 危险命令两段确认：第一次 Enter 只标记，第二次才执行。
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showRegister, setShowRegister] = useState(false);
  const [registerPath, setRegisterPath] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const skills = useMemo(() => library.data ?? [], [library.data]);
  const toolList = useMemo(() => tools.data ?? [], [tools.data]);
  const readOnlyById = useMemo(() => {
    const m = new Map<number, boolean>();
    for (const s of skills) {
      m.set(s.id, isReadOnlySkill(s.canonicalPath, toolList));
    }
    return m;
  }, [skills, toolList]);
  const deployableTools = useMemo(() => toolList.filter((x) => x.enabled && x.installed), [toolList]);
  const deployedBySkill = useMemo(() => {
    const m = new Map<number, Set<string>>();
    for (const x of targets.data ?? []) {
      const set = m.get(x.skillId) ?? new Set<string>();
      set.add(x.toolId);
      m.set(x.skillId, set);
    }
    return m;
  }, [targets.data]);
  const toolName = useMemo(() => {
    const m = new Map<string, ToolDto>();
    for (const x of toolList) {
      m.set(x.id, x);
    }
    return m;
  }, [toolList]);

  function go(view: ShellView) {
    onNavigate(view);
    onClose();
  }

  function invalidateCore() {
    void qc.invalidateQueries({ queryKey: queryKeys.library });
    void qc.invalidateQueries({ queryKey: queryKeys.targets });
  }

  // ------------------------------------------------------------------
  // 命令集：导航 + 全局操作 + 逐 skill / 逐 tool 写操作
  // ------------------------------------------------------------------
  const commands = useMemo<Command[]>(() => {
    const nav: Command[] = (
      [
        ["overview", "sidebar.overview"],
        ["library", "sidebar.library"],
        ["tools", "sidebar.tools"],
        ["doctor", "sidebar.doctor"],
        ["toolbox", "sidebar.toolbox"],
        ["settings", "sidebar.settings"],
      ] as [ShellView, string][]
    ).map(([view, key]) => ({
      id: `nav:${view}`,
      label: t(key),
      group: t("palette.group.navigate"),
      run: () => go(view),
    }));

    const actions: Command[] = [
      {
        id: "act:rescan",
        label: t("palette.rescan"),
        group: t("palette.group.action"),
        run: () => {
          scanStart()
            .then(() => {
              toast.show(t("palette.scanStarted"), "success");
              invalidateCore();
            })
            .catch((e) => toast.show(ipcError(e), "danger"));
        },
      },
      {
        id: "act:scan-cancel",
        label: t("palette.scanCancel"),
        group: t("palette.group.action"),
        run: () => {
          scanCancel()
            .then(() => toast.show(t("palette.scanCancelled"), "success"))
            .catch((e) => toast.show(ipcError(e), "danger"));
        },
      },
      {
        id: "act:doctor",
        label: t("palette.doctor"),
        group: t("palette.group.action"),
        run: () => {
          void qc.invalidateQueries({ queryKey: queryKeys.doctor });
          go("doctor");
        },
      },
      {
        id: "act:security-all",
        label: t("palette.securityAll"),
        group: t("palette.group.action"),
        run: () => {
          setBusy(true);
          void Promise.all(skills.map((s) => securityScan(s.id)))
            .then((rs) => {
              toast.show(t("palette.securityAllToast", { n: rs.length }), "success");
              void qc.invalidateQueries({ queryKey: queryKeys.securityOverview });
            })
            .catch((e) => toast.show(ipcError(e), "danger"))
            .finally(() => setBusy(false));
        },
      },
      {
        id: "act:open-central",
        label: t("overview.openCentralDir"),
        group: t("palette.group.action"),
        run: () => openPath.mutate("central", { onError: (e) => toast.show(ipcError(e), "danger") }),
      },
      {
        id: "act:toggle-lang",
        label: t("palette.toggleLang"),
        group: t("palette.group.action"),
        run: () => setLang(lang === "zh-CN" ? "en" : "zh-CN"),
      },
      {
        id: "act:register",
        label: t("palette.register"),
        group: t("palette.group.action"),
        run: () => {
          onClose();
          setShowRegister(true);
        },
      },
    ];

    const skillCmds: Command[] = [];
    for (const s of skills) {
      const readOnly = readOnlyById.get(s.id) ?? false;
      const deployed = deployedBySkill.get(s.id) ?? new Set<string>();
      if (!readOnly) {
        for (const tool of deployableTools) {
          if (!deployed.has(tool.id)) {
            skillCmds.push({
              id: `deploy:${s.id}:${tool.id}`,
              label: t("palette.deployTo", { skill: s.name, tool: tool.displayName }),
              group: t("palette.group.skill"),
              keywords: tool.id,
              run: () =>
                deployApply.mutate(
                  { skillId: s.id, tools: [tool.id], mode: "symlink", overwrite: false, dryRun: false },
                  {
                    onSuccess: () => toast.show(t("common.deployToast", { tool: tool.displayName }), "success"),
                    onError: (e) => toast.show(ipcError(e), "danger"),
                  },
                ),
            });
          }
        }
      }
      for (const toolId of deployed) {
        const name = toolName.get(toolId)?.displayName ?? toolId;
        skillCmds.push({
          id: `revoke:${s.id}:${toolId}`,
          label: t("palette.undeployFrom", { skill: s.name, tool: name }),
          group: t("palette.group.skill"),
          keywords: toolId,
          run: () =>
            deployRevoke.mutate(
              { skillId: s.id, tools: [toolId], dryRun: false },
              {
                onSuccess: () => toast.show(t("common.revokeToast", { tool: name }), "success"),
                onError: (e) => toast.show(ipcError(e), "danger"),
              },
            ),
        });
        // deploy_set_mode：每个已部署 (skill, tool) 一对 symlink/copy 命令。
        const setMode = (mode: "symlink" | "copy") => () =>
          deploySetMode(
            { skillId: s.id, tool: toolId, mode, dryRun: false },
          )
            .then(() => {
              invalidateCore();
              toast.show(
                mode === "symlink"
                  ? t("palette.modeSwitchedLink", { tool: name })
                  : t("palette.modeSwitchedCopy", { tool: name }),
                "success",
              );
            })
            .catch((e: unknown) => toast.show(ipcError(e), "danger"));
        skillCmds.push({
          id: `mode:${s.id}:${toolId}:symlink`,
          label: t("palette.setModeSymlink", { skill: s.name, tool: name }),
          group: t("palette.group.skill"),
          keywords: toolId,
          run: setMode("symlink"),
        });
        skillCmds.push({
          id: `mode:${s.id}:${toolId}:copy`,
          label: t("palette.setModeCopy", { skill: s.name, tool: name }),
          group: t("palette.group.skill"),
          keywords: toolId,
          run: setMode("copy"),
        });
      }
      skillCmds.push({
        id: `delete:${s.id}`,
        label: t("palette.deleteSkill", { skill: s.name }),
        group: t("palette.group.skill"),
        danger: true,
        run: () =>
          removeSkill.mutate(
            { skillId: s.id, mode: "delete", dryRun: false },
            {
              onSuccess: () => toast.show(t("library.deleteToast", { skill: s.name }), "success"),
              onError: (e) => toast.show(ipcError(e), "danger"),
            },
          ),
      });
    }

    const toolCmds: Command[] = [];
    for (const x of toolList) {
      toolCmds.push({
        id: `tool-on:${x.id}`,
        label: t("palette.enableTool", { tool: x.displayName }),
        group: t("palette.group.tool"),
        keywords: x.id,
        run: () =>
          setToolEnabled.mutate(
            { toolId: x.id, enabled: true },
            { onError: (e) => toast.show(ipcError(e), "danger") },
          ),
      });
      toolCmds.push({
        id: `tool-off:${x.id}`,
        label: t("palette.disableTool", { tool: x.displayName }),
        group: t("palette.group.tool"),
        keywords: x.id,
        run: () =>
          setToolEnabled.mutate(
            { toolId: x.id, enabled: false },
            { onError: (e) => toast.show(ipcError(e), "danger") },
          ),
      });
    }

    return [...nav, ...actions, ...skillCmds, ...toolCmds];
    // eslint 不在本项目；t / toast / mutations 为稳定引用，不列依赖。
  }, [skills, toolList, targets.data, lang, t]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) {
      return commands;
    }
    return commands.filter((c) => `${c.label}\n${c.group}\n${c.keywords ?? ""}`.toLowerCase().includes(q));
  }, [commands, query]);

  /// 渲染与键盘导航共用同一截断集（最多 50 条），保证 active 索引一致。
  const displayed = useMemo(() => filtered.slice(0, 50), [filtered]);

  // ------------------------------------------------------------------
  // 键盘交互：↑↓ 选择、Enter 执行（危险命令两段确认）、Esc 关闭
  // ------------------------------------------------------------------
  useEffect(() => {
    if (open) {
      setQuery("");
      setActive(0);
      setConfirmId(null);
      // 打开时聚焦输入框（等弹层动画挂载）。
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  useEffect(() => {
    setActive(0);
    setConfirmId(null);
  }, [query]);

  function runActive() {
    const cmd = displayed[active];
    if (!cmd || busy) {
      return;
    }
    if (cmd.danger && confirmId !== cmd.id) {
      setConfirmId(cmd.id);
      return;
    }
    if (!cmd.id.startsWith("nav:") && !cmd.id.startsWith("act:register")) {
      onClose();
    }
    cmd.run();
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, displayed.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      runActive();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  // ------------------------------------------------------------------
  // 渲染
  // ------------------------------------------------------------------
  return (
    <>
      <Modal open={open} onClose={onClose} title="Ctrl+K">
        <div className="flex items-center gap-2 rounded-btn border border-line bg-surface px-2.5">
          <Search size={14} className="text-ink-faint" />
          <input
            ref={inputRef}
            value={query}
            disabled={busy}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={t("palette.placeholder")}
            className="h-9 flex-1 bg-transparent text-sm text-ink placeholder:text-ink-faint focus:outline-none"
          />
          {busy && <span className="text-xs text-ink-faint">{t("palette.busy")}</span>}
        </div>
        <div className="mt-2 max-h-72 overflow-y-auto">
          {displayed.length === 0 ? (
            <p className="py-6 text-center text-xs text-ink-faint">{t("palette.empty")}</p>
          ) : (
            displayed.map((cmd, i) => (
              <button
                key={cmd.id}
                type="button"
                disabled={busy}
                className={[
                  "flex w-full items-center gap-2 rounded-btn px-2.5 py-2 text-left text-sm transition-colors duration-120",
                  i === active ? "bg-primary-soft text-primary-active" : "text-ink-secondary hover:bg-surface",
                  cmd.danger ? "text-danger" : "",
                ].join(" ")}
                onMouseEnter={() => setActive(i)}
                onClick={() => {
                  setActive(i);
                  if (cmd.danger && confirmId !== cmd.id) {
                    setConfirmId(cmd.id);
                  } else {
                    if (!cmd.id.startsWith("nav:") && !cmd.id.startsWith("act:register")) {
                      onClose();
                    }
                    cmd.run();
                  }
                }}
              >
                <span className="w-20 shrink-0 truncate text-[10px] tracking-wide text-ink-faint">{cmd.group}</span>
                <span className="min-w-0 flex-1 truncate">
                  {cmd.label}
                  {cmd.danger && confirmId === cmd.id && (
                    <span className="ml-1.5 font-medium"> · {t("palette.deleteConfirm")}</span>
                  )}
                </span>
                {i === active && <CornerDownLeft size={13} className="shrink-0 text-ink-faint" />}
              </button>
            ))
          )}
        </div>
      </Modal>

      {/* 登记本地 Skill 目录（library_install_local，原地登记零移动） */}
      <Modal
        open={showRegister}
        onClose={() => setShowRegister(false)}
        title={t("palette.registerTitle")}
        footer={
          <>
            <Button onClick={() => setShowRegister(false)}>{t("common.cancel")}</Button>
            <Button
              variant="primary"
              disabled={install.isPending || registerPath.trim().length === 0}
              onClick={() =>
                install.mutate(
                  { path: registerPath.trim(), name: null, keepInPlace: true, dryRun: false },
                  {
                    onSuccess: () => {
                      toast.show(t("library.registeredToast"), "success");
                      setShowRegister(false);
                      setRegisterPath("");
                    },
                    onError: (e) => toast.show(ipcError(e), "danger"),
                  },
                )
              }
            >
              {t("palette.registerOk")}
            </Button>
          </>
        }
      >
        <input
          value={registerPath}
          onChange={(e) => setRegisterPath(e.target.value)}
          placeholder={t("palette.registerPathLabel")}
          className="h-9 w-full rounded-btn border border-line bg-surface px-3 text-sm text-ink placeholder:text-ink-faint focus:border-primary focus:outline-none"
        />
        <p className="mt-2 text-xs leading-5 text-ink-faint">{t("palette.registerHint")}</p>
      </Modal>
    </>
  );
}
