/// 工具页：按形态分组的 agent 工具列表，启用开关 + 部署目录展示 + 打开目录。

import { FolderOpen } from "lucide-react";
import { ipcError } from "../api/ipc";
import { useOpenPath, useSetToolEnabled, useTools } from "../api/queries";
import { Badge, Button, EmptyState, Skeleton, Toggle } from "../components/ui";
import { useToast } from "../components/Toast";
import type { ToolDto } from "../api/ipc";

const GROUP_LABEL: Record<ToolDto["formFactor"], string> = {
  cli: "命令行",
  ide: "IDE",
  editorPlugin: "编辑器插件",
  terminalTui: "终端 TUI",
  desktopAssistant: "桌面助手",
};

const GROUP_ORDER: ToolDto["formFactor"][] = ["cli", "ide", "editorPlugin", "terminalTui", "desktopAssistant"];

export default function ToolsPage() {
  const tools = useTools();
  const setEnabled = useSetToolEnabled();
  const openPath = useOpenPath();
  const toast = useToast();

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

  return (
    <div className="mx-auto w-full max-w-3xl px-8 py-8">
      <h1 className="text-lg font-semibold text-ink">工具</h1>
      <p className="mt-1 text-xs text-ink-faint">
        关闭的工具不参与扫描与部署。目录按钮在系统资源管理器中打开。
      </p>

      {tools.isLoading ? (
        <div className="mt-5 space-y-2">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </div>
      ) : (tools.data ?? []).length === 0 ? (
        <div className="mt-4 rounded-card border border-line bg-surface">
          <EmptyState title="未发现任何 AI 工具" description="安装 Claude Code 等工具后，重启应用即可自动识别。" />
        </div>
      ) : (
        GROUP_ORDER.map((ff) => {
          const group = (tools.data ?? []).filter((t) => t.formFactor === ff);
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
                    <span className="flex h-8 w-8 items-center justify-center rounded-[8px] bg-primary-soft text-sm font-semibold text-primary-active">
                      {t.icon || t.displayName.slice(0, 1)}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2 text-sm text-ink">
                        {t.displayName}
                        {t.installed ? (
                          <Badge tone="success" dot>
                            已安装
                          </Badge>
                        ) : (
                          <Badge tone="neutral" dot>
                            未安装
                          </Badge>
                        )}
                      </div>
                      <div className="truncate font-mono text-xs text-ink-faint">
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
        })
      )}
    </div>
  );
}
