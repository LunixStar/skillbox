import type { ReactNode } from "react";

interface TooltipProps {
  content: string;
  children: ReactNode;
}

export default function Tooltip({ content, children }: TooltipProps) {
  return (
    <span className="group relative inline-flex">
      {children}
      <span
        className={[
          "pointer-events-none absolute bottom-full left-1/2 z-50 mb-1.5 -translate-x-1/2 whitespace-nowrap",
          "rounded-[6px] bg-ink px-2 py-1 text-xs text-white",
          "opacity-0 transition-opacity duration-120 ease-skillbox group-hover:opacity-100",
        ].join(" ")}
        role="tooltip"
      >
        {content}
      </span>
    </span>
  );
}
