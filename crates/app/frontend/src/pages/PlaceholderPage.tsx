/// 占位页（体检 / 工具箱 / 设置，Phase 4）：诚实空态，不造假数据。

import { EmptyState } from "../components/ui";

interface PlaceholderPageProps {
  title: string;
  description: string;
  phase: string;
}

export default function PlaceholderPage({ title, description, phase }: PlaceholderPageProps) {
  return (
    <div className="flex flex-1 flex-col px-8 py-8">
      <h1 className="text-lg font-semibold text-ink">{title}</h1>
      <div className="mt-4 flex-1 rounded-card border border-line bg-surface">
        <EmptyState
          title={`${title}功能将在 ${phase} 提供`}
          description={description}
        />
      </div>
    </div>
  );
}
