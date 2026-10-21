/// 应用根：首启判定（settings `first_run.done`）→ 首次运行向导 / 主框架。

import HighlightBar from "./components/HighlightBar";
import { Skeleton } from "./components/ui";
import FirstRun from "./first-run/FirstRun";
import MainShell from "./shell/MainShell";
import { useFirstRunDone } from "./api/queries";

export default function App() {
  const firstRun = useFirstRunDone();

  return (
    <div className="flex h-screen flex-col bg-surface">
      {/* 顶部 2px 渐变高光条（§11.1，纯 CSS） */}
      <HighlightBar />
      {firstRun.isLoading ? (
        <div className="flex flex-1 items-center justify-center">
          <Skeleton className="h-24 w-96 rounded-card" />
        </div>
      ) : firstRun.data === "true" ? (
        <MainShell />
      ) : (
        <FirstRun />
      )}
    </div>
  );
}
