import { useState } from "react";
import IPCTester from "./dev/IPCTester";

export default function App() {
  const [view, setView] = useState<"home" | "dev">("home");

  return (
    <div className="flex h-screen flex-col bg-surface">
      {/* 顶部 2px 渐变高光条（§11.1，纯 CSS） */}
      <div className="h-[2px] w-full bg-gradient-to-r from-primary via-[#56C8F0] to-primary" />
      <button
        className="absolute right-3 top-3 rounded border border-line bg-white px-2 py-1 text-xs text-ink-secondary"
        onClick={() => setView(view === "home" ? "dev" : "home")}
      >
        {view === "home" ? "IPC Tester" : "返回"}
      </button>
      {view === "dev" ? (
        <IPCTester />
      ) : (
        <div className="flex flex-1 items-center justify-center">
          <div className="rounded-[10px] border border-line bg-white px-10 py-8 shadow-[0_1px_3px_rgba(16,40,80,0.06)]">
            <h1 className="text-xl font-semibold text-ink">SkillBox</h1>
            <p className="mt-1 text-sm text-ink-secondary">
              AI Agent Skills 桌面管理器
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
