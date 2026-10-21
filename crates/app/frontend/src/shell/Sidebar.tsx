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
  label: string;
  icon: ReactNode;
}

const MAIN_NAV: NavItem[] = [
  { key: "overview", label: "概览", icon: <LayoutDashboard size={16} /> },
  { key: "library", label: "技能库", icon: <LibraryIcon size={16} /> },
  { key: "tools", label: "工具", icon: <Bot size={16} /> },
  { key: "doctor", label: "体检", icon: <Stethoscope size={16} /> },
  { key: "toolbox", label: "工具箱", icon: <ToolboxIcon size={16} /> },
  { key: "settings", label: "设置", icon: <SettingsIcon size={16} /> },
];

const DEV_NAV: NavItem[] = [
  { key: "design", label: "设计系统", icon: <Palette size={16} /> },
  { key: "dev", label: "IPC Tester", icon: <FlaskConical size={16} /> },
];

interface SidebarProps {
  view: ShellView;
  onSelect: (view: ShellView) => void;
}

function NavButton({ item, active, onSelect }: { item: NavItem; active: boolean; onSelect: () => void }) {
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
      {item.label}
    </button>
  );
}

export default function Sidebar({ view, onSelect }: SidebarProps) {
  const openPath = useOpenPath();
  const toast = useToast();

  return (
    <aside className="flex w-48 shrink-0 flex-col border-r border-line bg-page">
      <div className="px-4 pt-5 pb-3">
        <div className="text-sm font-semibold text-ink">SkillBox</div>
        <div className="mt-0.5 text-xs text-ink-faint">AI Agent Skills 管理器</div>
      </div>
      <nav className="flex-1 space-y-0.5 px-2">
        {MAIN_NAV.map((item) => (
          <NavButton key={item.key} item={item} active={view === item.key} onSelect={() => onSelect(item.key)} />
        ))}
      </nav>
      <div className="space-y-0.5 border-t border-line px-2 py-2">
        <div className="px-3 pt-1 pb-1 text-[10px] tracking-wider text-ink-faint">开发者</div>
        {DEV_NAV.map((item) => (
          <NavButton key={item.key} item={item} active={view === item.key} onSelect={() => onSelect(item.key)} />
        ))}
      </div>
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
          打开中央库
        </button>
      </div>
    </aside>
  );
}
