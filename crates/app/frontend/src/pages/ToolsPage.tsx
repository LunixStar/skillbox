/// 工具页（§4 / §11.3）：按 FormFactor 分组（CLI / IDE·桌面端 / 编辑器插件 /
/// 终端 TUI / 桌面助手）。已检测工具带启停开关 + 打开目录；
/// 未检测工具折叠进"未检测到"区（默认收起，只读展示）。

import { useState } from "react";
import { ChevronDown, ChevronRight, FolderOpen } from "lucide-react";
import { ipcError, type ToolDto } from "../api/ipc";
import { useOpenPath, useSetToolEnabled, useTools } from "../api/queries";
import { Badge, Button, EmptyState, Skeleton, Toggle } from "../components/ui";
import { useToast } from "../components/Toast";

type FormFactor = ToolDto["formFactor"];

const GROUP_LABEL: Record<FormFactor, string> = {
  cli: "命令行（CLI）",
  ide: "IDE · 桌面端",
  editorPlugin: "编辑器插件",
  terminalTui: "终端 TUI",
  desktopAssistant: "桌面助手",
};

const FORM_SHORT: Record<FormFactor, string> = {
  cli: "CLI",
  ide: "IDE",
  editorPlugin: "插件",
  terminalTui: "TUI",
  desktopAssistant: "助手",
};

const GROUP_ORDER: FormFactor[] = ["cli", "ide", "editorPlugin", "terminalTui", "desktopAssistant"];

export default function ToolsPage() {
  const tools = useTools();
  const setEnabled = useSetToolEnabled();
  const openPath = useOpenPath();
  const toast = useToast();
  const [showUndetected, setShowUndetected] = useState(false);

  if (tools.isError) {
    return (
      <div className="px-8 py-8">
        <h1 className="text-lg font-semibold text-ink">工具</h1>
        <div className="mt-4 rounded-card border border-line bg-surface">
          <EmptyState
            title="无法加载工具列表"
            description={ipcError(tools.error)}
            action={
              <Button variant="primary" size="sm" onClick={() => tools.refetch()}>
                重试
              </Button>
            }
          />
        </div>
      </div>
    );
  }

  const all = tools.data ?? [];
  const installed = all.filter((t) => t.installed);
  const undetected = all.filter((t) => !t.installed);

  return (
    <div className="mx-auto w-full max-w-3xl px-8 py-8">
      <h1 className="text-lg font-semibold text-ink">工具</h1>
      <p className="mt-1 text-xs text-ink-faint">
        关闭的工具不参与扫描与部署。目录按钮在系统文件管理器中打开。
      </p>

      {tools.isLoading ? (
        <div className="mt-5 space-y-2">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </div>
      ) : all.length === 0 ? (
        <div className="mt-4 rounded-card border border-line bg-surface">
          <EmptyState title="未发现任何 AI 工具" description="安装 Claude Code 等工具后，重启应用即可自动识别。" />
        </div>
      ) : (
        <>
          {GROUP_ORDER.map((ff) => {
            const group = installed.filter((t) => t.formFactor === ff);
            if (group.length === 0) {
              return null;
            }
            return (
              <div key={ff} className="mt-6">
                <div className="text-xs font-semibold tracking-wide text-ink-faint">
                  {GROUP_LABEL[ff]}（{group.length}）
                </div>
                <div className="mt-2 space-y-1.5">
                  {group.map((t) => (
                    <div
                      key={t.id}
                      className="flex items-center gap-3 rounded-card border border-line bg-surface px-4 py-2.5"
                    >
                      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-primary-soft text-sm font-semibold text-primary-active">
                        {t.icon || t.displayName.slice(0, 1)}
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 text-sm text-ink">
                          {t.displayName}
                          <Badge tone="neutral">{FORM_SHORT[t.formFactor]}</Badge>
                          <Badge tone="success" dot>
                            已检测
                          </Badge>
                        </div>
                        <div className="truncate font-mono text-xs text-ink-faint" title={t.deployDir ?? ""}>
                          {t.deployDir ?? "—"}
                        </div>
                      </div>
                      {t.deployDir && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() =>
                            openPath.mutate(`tool:${t.id}`, {
                              onError: (e) => toast.show(ipcError(e), "danger"),
                            })
                          }
                        >
                          <FolderOpen size={14} />
                          目录
                        </Button>
                      )}
                      <Toggle
                        checked={t.enabled}
                        onChange={(next) =>
                          setEnabled.mutate(
                            { toolId: t.id, enabled: next },
                            { onError: (e) => toast.show(ipcError(e), "danger") },
                          )
                        }
                        label={t.enabled ? "启用" : "停用"}
                      />
                    </div>
                  ))}
                </div>
              </div>
            );
          })}

          {undetected.length > 0 && (
            <div className="mt-6">
              <button
                type="button"
                className="flex w-full items-center gap-2 rounded-card border border-line bg-surface px-4 py-2.5 text-left text-sm text-ink-secondary transition-transform duration-120 ease-skillbox hover:text-ink"
                onClick={() => setShowUndetected((v) => !v)}
                aria-expanded={showUndetected}
              >
                {showUndetected ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                <span className="font-medium">未检测到（{undetected.length}）</span>
                <span className="truncate text-xs text-ink-faint">
                  该工具的配置目录不存在——启动一次该工具即可被检测到
                </span>
              </button>
              {showUndetected && (
                <div className="mt-1.5 space-y-1.5 sb-fade-enter">
                  {GROUP_ORDER.map((ff) => {
                    const group = undetected.filter((t) => t.formFactor === ff);
                    if (group.length === 0) {
                      return null;
                    }
                    return (
                      <div key={ff}>
                        <div className="px-1 pt-1 text-xs font-semibold tracking-wide text-ink-faint">
                          {GROUP_LABEL[ff]}
                        </div>
                        <div className="mt-1 space-y-1.5">
                          {group.map((t) => (
                            <div
                              key={t.id}
                              className="flex items-center gap-3 rounded-card border border-dashed border-line bg-surface px-4 py-2 opacity-70"
                            >
                              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-surface text-sm font-semibold text-ink-faint">
                                {t.icon || t.displayName.slice(0, 1)}
                              </span>
                              <div className="min-w-0 flex-1">
                                <div className="flex items-center gap-2 text-sm text-ink">
                                  {t.displayName}
                                  <Badge tone="neutral">{FORM_SHORT[t.formFactor]}</Badge>
                                </div>
                                <div className="truncate font-mono text-xs text-ink-faint" title={t.deployDir ?? ""}>
                                  {t.deployDir ?? "—"}
                                </div>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
