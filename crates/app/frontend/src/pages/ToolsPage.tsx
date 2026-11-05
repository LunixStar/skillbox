/// 工具页（§4 / §11.3）：按 FormFactor 分组（CLI / IDE·桌面端 / 编辑器插件 /
/// 终端 TUI / 桌面助手）。已检测工具带启停开关 + 打开目录；
/// 未检测工具折叠进"未检测到"区（默认收起，只读展示）。

import { useState, type CSSProperties } from "react";
import { ChevronDown, ChevronRight, FolderOpen } from "lucide-react";
import { ipcError, type ToolDto } from "../api/ipc";
import { useOpenPath, useSetToolEnabled, useTools } from "../api/queries";
import { Badge, Button, EmptyState, Skeleton, Toggle } from "../components/ui";
import ToolIcon from "../components/ToolIcon";
import { useToast } from "../components/Toast";
import { useI18n } from "../i18n";

type FormFactor = ToolDto["formFactor"];

const GROUP_LABEL_KEY: Record<FormFactor, string> = {
  cli: "tools.group.cli",
  ide: "tools.group.ide",
  editorPlugin: "tools.group.editorPlugin",
  terminalTui: "tools.group.terminalTui",
  desktopAssistant: "tools.group.desktopAssistant",
};

const FORM_SHORT_KEY: Record<FormFactor, string> = {
  cli: "tools.short.cli",
  ide: "tools.short.ide",
  editorPlugin: "tools.short.editorPlugin",
  terminalTui: "tools.short.terminalTui",
  desktopAssistant: "tools.short.desktopAssistant",
};

const GROUP_ORDER: FormFactor[] = ["cli", "ide", "editorPlugin", "terminalTui", "desktopAssistant"];

export default function ToolsPage() {
  const tools = useTools();
  const setEnabled = useSetToolEnabled();
  const openPath = useOpenPath();
  const toast = useToast();
  const { t } = useI18n();
  const [showUndetected, setShowUndetected] = useState(false);

  if (tools.isError) {
    return (
      <div className="px-8 py-8">
        <h1 className="text-lg font-semibold text-ink">{t("tools.title")}</h1>
        <div className="mt-4 rounded-card border border-line bg-surface">
          <EmptyState
            title={t("tools.errorTitle")}
            description={ipcError(tools.error)}
            action={
              <Button variant="primary" size="sm" onClick={() => tools.refetch()}>
                {t("common.retry")}
              </Button>
            }
          />
        </div>
      </div>
    );
  }

  const all = tools.data ?? [];
  const installed = all.filter((x) => x.installed);
  const undetected = all.filter((x) => !x.installed);

  return (
    <div className="mx-auto w-full max-w-3xl px-8 py-8">
      <h1 className="text-lg font-semibold text-ink">{t("tools.title")}</h1>
      <p className="mt-1 text-xs text-ink-faint">{t("tools.subtitle")}</p>

      {tools.isLoading ? (
        <div className="mt-5 space-y-2">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </div>
      ) : all.length === 0 ? (
        <div className="mt-4 rounded-card border border-line bg-surface">
          <EmptyState title={t("tools.emptyTitle")} description={t("tools.emptyDesc")} />
        </div>
      ) : (
        <>
          {GROUP_ORDER.map((ff) => {
            const group = installed.filter((x) => x.formFactor === ff);
            if (group.length === 0) {
              return null;
            }
            return (
              <div key={ff} className="mt-6">
                <div className="text-xs font-semibold tracking-wide text-ink-faint">
                  {t(GROUP_LABEL_KEY[ff])}（{group.length}）
                </div>
                <div className="mt-2 space-y-1.5">
                  {group.map((x, i) => (
                    <div
                      key={x.id}
                      className="flex items-center gap-3 rounded-card border border-line bg-surface px-4 py-2.5 sb-fade-up"
                      style={{ "--sb-i": i } as CSSProperties}
                    >
                      <ToolIcon icon={x.icon} name={x.displayName} size={32} />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 text-sm text-ink">
                          {x.displayName}
                          <Badge tone="neutral">{t(FORM_SHORT_KEY[x.formFactor])}</Badge>
                          <Badge tone="success" dot>
                            {t("tools.detected")}
                          </Badge>
                        </div>
                        <div className="truncate font-mono text-xs text-ink-faint" title={x.deployDir ?? ""}>
                          {x.deployDir ?? "—"}
                        </div>
                      </div>
                      {x.deployDir && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() =>
                            openPath.mutate(`tool:${x.id}`, {
                              onError: (e) => toast.show(ipcError(e), "danger"),
                            })
                          }
                        >
                          <FolderOpen size={14} />
                          {t("tools.folder")}
                        </Button>
                      )}
                      <Toggle
                        checked={x.enabled}
                        onChange={(next) =>
                          setEnabled.mutate(
                            { toolId: x.id, enabled: next },
                            { onError: (e) => toast.show(ipcError(e), "danger") },
                          )
                        }
                        label={x.enabled ? t("tools.enabled") : t("tools.disabled")}
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
                <span className="font-medium">{t("tools.undetected", { n: undetected.length })}</span>
                <span className="truncate text-xs text-ink-faint">{t("tools.undetectedHint")}</span>
              </button>
              {showUndetected && (
                <div className="mt-1.5 space-y-1.5 sb-fade-enter">
                  {GROUP_ORDER.map((ff) => {
                    const group = undetected.filter((x) => x.formFactor === ff);
                    if (group.length === 0) {
                      return null;
                    }
                    return (
                      <div key={ff}>
                        <div className="px-1 pt-1 text-xs font-semibold tracking-wide text-ink-faint">
                          {t(GROUP_LABEL_KEY[ff])}
                        </div>
                        <div className="mt-1 space-y-1.5">
                          {group.map((x) => (
                            <div
                              key={x.id}
                              className="flex items-center gap-3 rounded-card border border-dashed border-line bg-surface px-4 py-2 opacity-70"
                            >
                              <ToolIcon icon={x.icon} name={x.displayName} size={32} muted />
                              <div className="min-w-0 flex-1">
                                <div className="flex items-center gap-2 text-sm text-ink">
                                  {x.displayName}
                                  <Badge tone="neutral">{t(FORM_SHORT_KEY[x.formFactor])}</Badge>
                                </div>
                                <div className="truncate font-mono text-xs text-ink-faint" title={x.deployDir ?? ""}>
                                  {x.deployDir ?? "—"}
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
