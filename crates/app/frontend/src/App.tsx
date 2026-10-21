import { useState } from "react";
import HighlightBar from "./components/HighlightBar";
import DesignSystemPage from "./dev/DesignSystemPage";
import IPCTester from "./dev/IPCTester";

type View = "home" | "design" | "dev";

export default function App() {
  const [view, setView] = useState<View>("home");

  return (
    <div className="flex h-screen flex-col bg-surface">
      {/* 顶部 2px 渐变高光条（§11.1，纯 CSS） */}
      <HighlightBar />
      <div className="absolute right-3 top-3 z-10 flex gap-1.5">
        {(
          [
            ["home", "首页"],
            ["design", "设计系统"],
            ["dev", "IPC Tester"],
          ] as [View, string][]
        ).map(([key, label]) => (
          <button
            key={key}
            className={`rounded-btn border px-2.5 py-1 text-xs ${
              view === key
                ? "border-primary bg-primary-soft text-primary-active"
                : "border-line bg-page text-ink-secondary hover:bg-surface"
            }`}
            onClick={() => setView(key)}
          >
            {label}
          </button>
        ))}
      </div>
      {view === "dev" ? (
        <IPCTester />
      ) : view === "design" ? (
        <DesignSystemPage />
      ) : (
        <div className="flex flex-1 items-center justify-center">
          <div className="sb-card sb-card--interactive px-10 py-8">
            <h1 className="text-xl font-semibold text-ink">SkillBox</h1>
            <p className="mt-1 text-sm text-ink-secondary">AI Agent Skills 桌面管理器</p>
          </div>
        </div>
      )}
    </div>
  );
}
