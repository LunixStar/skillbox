import type { ReactNode } from "react";

interface CardProps {
  children: ReactNode;
  className?: string;
  interactive?: boolean;
  onClick?: () => void;
}

export default function Card({
  children,
  className = "",
  interactive = true,
  onClick,
}: CardProps) {
  return (
    <div
      className={[
        "sb-card",
        interactive ? "sb-card--interactive" : "",
        onClick ? "cursor-pointer" : "",
        className,
      ].join(" ")}
      onClick={onClick}
    >
      {children}
    </div>
  );
}
