import type { ReactNode } from "react";

type Tone = "blue" | "neutral" | "success" | "danger";

interface BadgeProps {
  tone?: Tone;
  dot?: boolean;
  children: ReactNode;
}

const toneClass: Record<Tone, string> = {
  blue: "bg-primary-soft text-primary-active",
  neutral: "bg-surface text-ink-secondary",
  success: "bg-[#EAF6F0] text-success",
  danger: "bg-[#FDEEEC] text-danger",
};

const dotClass: Record<Tone, string> = {
  blue: "bg-primary",
  neutral: "bg-ink-faint",
  success: "bg-success",
  danger: "bg-danger",
};

export default function Badge({ tone = "neutral", dot = false, children }: BadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-[6px] px-1.5 py-0.5 text-xs font-medium ${toneClass[tone]}`}
    >
      {dot && <span className={`h-1.5 w-1.5 rounded-full ${dotClass[tone]}`} />}
      {children}
    </span>
  );
}
