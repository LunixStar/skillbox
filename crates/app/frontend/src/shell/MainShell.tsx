/// 主框架：侧边栏 + 视图切换（桌面应用，简单 useState 切换足够，不引入路由）。

import { useState } from "react";
import DesignSystemPage from "../dev/DesignSystemPage";
import IPCTester from "../dev/IPCTester";
import LibraryPage from "../pages/LibraryPage";
import OverviewPage from "../pages/OverviewPage";
import PlaceholderPage from "../pages/PlaceholderPage";
import ToolsPage from "../pages/ToolsPage";
import Sidebar, { type ShellView } from "./Sidebar";

const PLACEHOLDERS: Partial<Record<ShellView, { title: string; description: string; phase: string }>> = {
  doctor: {
    title: "体检",
    description: "对中央库与部署目标做一致性体检，发现漂移与损坏链接后给出修复建议。",
    phase: "Phase 4",
  },
  toolbox: {
    title: "工具箱",
    description: "安全扫描、上下文成本分析与日志 / 缓存管理。回收站恢复也在这里。",
    phase: "Phase 4",
  },
  settings: {
    title: "设置",
    description: "中央库位置、扫描黑名单、语言等应用设置。",
    phase: "Phase 4",
  },
};

export default function MainShell() {
  const [view, setView] = useState<ShellView>("overview");

  return (
    <div className="flex flex-1 overflow-hidden">
      <Sidebar view={view} onSelect={setView} />
      <main className="flex min-w-0 flex-1 flex-col overflow-y-auto bg-page">
        {view === "overview" && <OverviewPage onNavigate={setView} />}
        {view === "library" && <LibraryPage />}
        {view === "tools" && <ToolsPage />}
        {view === "doctor" && <PlaceholderPage {...PLACEHOLDERS.doctor!} />}
        {view === "toolbox" && <PlaceholderPage {...PLACEHOLDERS.toolbox!} />}
        {view === "settings" && <PlaceholderPage {...PLACEHOLDERS.settings!} />}
        {view === "design" && <DesignSystemPage />}
        {view === "dev" && <IPCTester />}
      </main>
    </div>
  );
}
