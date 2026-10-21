import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

const variantClass: Record<Variant, string> = {
  primary: "bg-primary text-white hover:bg-primary-hover active:bg-primary-active",
  secondary:
    "border border-line bg-page text-ink hover:bg-surface active:bg-line",
  ghost: "text-ink-secondary hover:bg-surface hover:text-ink active:bg-line",
  danger: "bg-danger text-white hover:opacity-90 active:opacity-75",
};

const sizeClass: Record<Size, string> = {
  sm: "h-7 px-2.5 text-xs",
  md: "h-8 px-3.5 text-sm",
};

export default function Button({
  variant = "secondary",
  size = "md",
  className = "",
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={[
        "inline-flex select-none items-center justify-center gap-1.5 rounded-btn font-medium",
        "transition-transform duration-120 ease-skillbox active:scale-[0.97]",
        "disabled:pointer-events-none disabled:opacity-40",
        variantClass[variant],
        sizeClass[size],
        className,
      ].join(" ")}
      {...rest}
    />
  );
}
