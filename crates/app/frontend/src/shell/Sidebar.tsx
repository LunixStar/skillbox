/// 主框架侧边栏（§11.3）：概览 / 技能库★ / 工具 / 体检 / 工具箱 / 设置，
/// 底部为开发者入口与中央库快捷方式。选中项：3px 蓝色左指示条 + primary-soft 底。

import {
  Bot,
  FlaskConical,
  FolderOpen,
  LayoutDashboard,
  Library as LibraryIcon,
  Palette,
  Settings as SettingsIcon,
  Stethoscope,
  Toolbox as ToolboxIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { ipcError } from "../api/ipc";
import { useOpenPath } from "../api/queries";
import { useToast } from "../components/Toast";
import { useI18n } from "../i18n";

export type ShellView =
  | "overview"
  | "library"
  | "tools"
  | "doctor"
  | "toolbox"
  | "settings"
  | "design"
  | "dev";

interface NavItem {
  key: ShellView;
  labelKey: string;
  icon: ReactNode;
}

const MAIN_NAV: NavItem[] = [
  { key: "overview", labelKey: "sidebar.overview", icon: <LayoutDashboard size={16} /> },
  { key: "library", labelKey: "sidebar.library", icon: <LibraryIcon size={16} /> },
  { key: "tools", labelKey: "sidebar.tools", icon: <Bot size={16} /> },
  { key: "doctor", labelKey: "sidebar.doctor", icon: <Stethoscope size={16} /> },
  { key: "toolbox", labelKey: "sidebar.toolbox", icon: <ToolboxIcon size={16} /> },
  { key: "settings", labelKey: "sidebar.settings", icon: <SettingsIcon size={16} /> },
];

const DEV_NAV: NavItem[] = [
  { key: "design", labelKey: "sidebar.design", icon: <Palette size={16} /> },
  { key: "dev", labelKey: "sidebar.ipcTester", icon: <FlaskConical size={16} /> },
];

interface SidebarProps {
  view: ShellView;
  onSelect: (view: ShellView) => void;
  /// dev 入口（设计系统 / IPC Tester）仅在 dev 构建可见（MainShell 路由守卫）。
  showDev?: boolean;
}

function NavButton({ item, active, onSelect }: { item: NavItem; active: boolean; onSelect: () => void }) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      onClick={onSelect}
      className={[
        "relative flex w-full items-center gap-2.5 rounded-r-btn px-3 py-2 text-sm",
        "transition-colors duration-120",
        active
          ? "bg-primary-soft font-medium text-primary-active"
          : "text-ink-secondary hover:bg-surface hover:text-ink",
      ].join(" ")}
    >
      {active && <span className="absolute left-0 top-1/2 h-5 w-[3px] -translate-y-1/2 rounded-r-full bg-primary" />}
      {item.icon}
      {t(item.labelKey)}
    </button>
  );
}

export default function Sidebar({ view, onSelect, showDev = false }: SidebarProps) {
  const openPath = useOpenPath();
  const toast = useToast();
  const { t } = useI18n();

  return (
    <aside className="flex w-48 shrink-0 flex-col border-r border-line bg-page">
      <div className="px-4 pt-5 pb-3">
        <div className="text-sm font-semibold text-ink">SkillBox</div>
        <div className="mt-0.5 text-xs text-ink-faint">{t("sidebar.subtitle")}</div>
      </div>
      <nav className="flex-1 space-y-0.5 px-2">
        {MAIN_NAV.map((item) => (
          <NavButton key={item.key} item={item} active={view === item.key} onSelect={() => onSelect(item.key)} />
        ))}
      </nav>
      {showDev && (
        <div className="space-y-0.5 border-t border-line px-2 py-2">
          <div className="px-3 pt-1 pb-1 text-[10px] tracking-wider text-ink-faint">{t("sidebar.devLabel")}</div>
          {DEV_NAV.map((item) => (
            <NavButton key={item.key} item={item} active={view === item.key} onSelect={() => onSelect(item.key)} />
          ))}
        </div>
      )}
      <div className="border-t border-line px-2 py-2">
        <button
          type="button"
          className="flex w-full items-center gap-2 rounded-r-btn px-3 py-2 text-sm text-ink-secondary transition-colors duration-120 hover:bg-surface hover:text-ink"
          onClick={() =>
            openPath.mutate("central", {
              onError: (e) => toast.show(ipcError(e), "danger"),
            })
          }
        >
          <FolderOpen size={16} />
          {t("common.openCentralLibrary")}
        </button>
      </div>
    </aside>
  );
}
