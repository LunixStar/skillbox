/// 自建虚拟滚动列表（§11 DoD：>300 条时使用，约 80 行，禁用第三方库）。
/// 原理：容器总高 = items.length × itemHeight 撑开滚动区，
/// 按 scrollTop 只渲染可见窗口（± overscan）的行，行用 absolute 定位。

import { useRef, useState, type ReactNode } from "react";

interface VirtualListProps<T> {
  items: T[];
  itemHeight: number;
  height: number;
  overscan?: number;
  renderItem: (item: T, index: number) => ReactNode;
}

export default function VirtualList<T>({
  items,
  itemHeight,
  height,
  overscan = 6,
  renderItem,
}: VirtualListProps<T>) {
  const [scrollTop, setScrollTop] = useState(0);
  const frame = useRef<HTMLDivElement>(null);

  const total = items.length;
  const start = Math.max(0, Math.floor(scrollTop / itemHeight) - overscan);
  const end = Math.min(total, Math.ceil((scrollTop + height) / itemHeight) + overscan);
  const visible = items.slice(start, end);

  return (
    <div
      ref={frame}
      className="relative overflow-y-auto"
      style={{ height }}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
    >
      <div style={{ height: total * itemHeight, position: "relative" }}>
        {visible.map((item, i) => (
          <div
            key={start + i}
            style={{
              position: "absolute",
              top: (start + i) * itemHeight,
              left: 0,
              right: 0,
              height: itemHeight,
            }}
          >
            {renderItem(item, start + i)}
          </div>
        ))}
      </div>
    </div>
  );
}
