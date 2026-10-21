/// 主框架：侧边栏 + 视图切换（桌面应用，简单 useState 切换足够，不引入路由）。

import { useState } from "react";
import DesignSystemPage from "../dev/DesignSystemPage";
import IPCTester from "../dev/IPCTester";
import DoctorPage from "../pages/DoctorPage";
import LibraryPage from "../pages/LibraryPage";
import OverviewPage from "../pages/OverviewPage";
import SettingsPage from "../pages/SettingsPage";
import ToolboxPage from "../pages/ToolboxPage";
import ToolsPage from "../pages/ToolsPage";
import Sidebar, { type ShellView } from "./Sidebar";

export default function MainShell() {
  const [view, setView] = useState<ShellView>("overview");

  return (
    <div className="flex flex-1 overflow-hidden">
      <Sidebar view={view} onSelect={setView} />
      <main className="flex min-w-0 flex-1 flex-col overflow-y-auto bg-page">
        {view === "overview" && <OverviewPage onNavigate={setView} />}
        {view === "library" && <LibraryPage />}
        {view === "tools" && <ToolsPage />}
        {view === "doctor" && <DoctorPage />}
        {view === "toolbox" && <ToolboxPage />}
        {view === "settings" && <SettingsPage />}
        {view === "design" && <DesignSystemPage />}
        {view === "dev" && <IPCTester />}
      </main>
    </div>
  );
}
