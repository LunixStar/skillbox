import type { CSSProperties, ReactNode } from "react";

interface CardProps {
  children: ReactNode;
  className?: string;
  interactive?: boolean;
  onClick?: () => void;
  style?: CSSProperties;
}

export default function Card({
  children,
  className = "",
  interactive = true,
  onClick,
  style,
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
      style={style}
    >
      {children}
    </div>
  );
}
