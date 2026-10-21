/// 概览页：库规模统计 + 快捷操作。

import { Link2, Library as LibraryIcon, Bot } from "lucide-react";
import { ipcError } from "../api/ipc";
import { useLibrary, useOpenPath, useTargets, useTools } from "../api/queries";
import { Button, Card, Skeleton } from "../components/ui";
import { useToast } from "../components/Toast";
import type { ShellView } from "../shell/Sidebar";

interface OverviewPageProps {
  onNavigate: (view: ShellView) => void;
}

export default function OverviewPage({ onNavigate }: OverviewPageProps) {
  const library = useLibrary();
  const targets = useTargets();
  const tools = useTools();
  const openPath = useOpenPath();
  const toast = useToast();

  const loading = library.isLoading || targets.isLoading || tools.isLoading;
  const enabledTools = (tools.data ?? []).filter((t) => t.enabled && t.installed).length;

  const stats: { label: string; value: number | null; icon: React.ReactNode }[] = [
    { label: "技能总数", value: library.data?.length ?? null, icon: <LibraryIcon size={16} /> },
    { label: "已部署目标", value: targets.data?.length ?? null, icon: <Link2 size={16} /> },
    { label: "已启用工具", value: loading ? null : enabledTools, icon: <Bot size={16} /> },
  ];

  return (
    <div className="mx-auto w-full max-w-3xl px-8 py-8">
      <h1 className="text-lg font-semibold text-ink">概览</h1>
      <div className="mt-5 grid grid-cols-3 gap-3">
        {stats.map((s) => (
          <Card key={s.label} className="px-4 py-4">
            <div className="flex items-center gap-1.5 text-xs text-ink-secondary">
              {s.icon}
              {s.label}
            </div>
            <div className="mt-1.5 text-2xl font-semibold text-ink">
              {s.value === null ? <Skeleton className="h-8 w-12" /> : s.value}
            </div>
          </Card>
        ))}
      </div>

      <h2 className="mt-8 text-sm font-semibold text-ink">快捷操作</h2>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button variant="primary" onClick={() => onNavigate("library")}>
          打开技能库
        </Button>
        <Button onClick={() => onNavigate("tools")}>管理工具</Button>
        <Button
          onClick={() =>
            openPath.mutate("central", { onError: (e) => toast.show(ipcError(e), "danger") })
          }
        >
          打开中央库目录
        </Button>
      </div>
    </div>
  );
}
