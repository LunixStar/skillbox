/// 主框架：侧边栏 + 视图切换（桌面应用，简单 useState 切换足够，不引入路由）。
/// Ctrl+K 命令面板（§11.3）全局挂载；dev 页（设计系统 / IPC Tester）仅在
/// dev 构建可见（import.meta.env.DEV 路由守卫，release 构建不可达）。

import { useEffect, useState } from "react";
import DesignSystemPage from "../dev/DesignSystemPage";
import IPCTester from "../dev/IPCTester";
import CommandPalette from "../components/CommandPalette";
import DoctorPage from "../pages/DoctorPage";
import LibraryPage from "../pages/LibraryPage";
import OverviewPage from "../pages/OverviewPage";
import SettingsPage from "../pages/SettingsPage";
import ToolboxPage from "../pages/ToolboxPage";
import ToolsPage from "../pages/ToolsPage";
import Sidebar, { type ShellView } from "./Sidebar";

/// dev 页路由守卫：Vite 静态替换，release 构建恒为 false。
const IS_DEV = import.meta.env.DEV;

export default function MainShell() {
  const [view, setView] = useState<ShellView>("overview");
  const [paletteOpen, setPaletteOpen] = useState(false);

  // Ctrl+K / Cmd+K 呼出命令面板（§11.3）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex flex-1 overflow-hidden">
      <Sidebar view={view} onSelect={setView} showDev={IS_DEV} />
      <main className="flex min-w-0 flex-1 flex-col overflow-y-auto bg-page">
        {view === "overview" && <OverviewPage onNavigate={setView} />}
        {view === "library" && <LibraryPage />}
        {view === "tools" && <ToolsPage />}
        {view === "doctor" && <DoctorPage />}
        {view === "toolbox" && <ToolboxPage />}
        {view === "settings" && <SettingsPage />}
        {IS_DEV && view === "design" && <DesignSystemPage />}
        {IS_DEV && view === "dev" && <IPCTester />}
      </main>
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={(v) => setView(v)}
      />
    </div>
  );
}
