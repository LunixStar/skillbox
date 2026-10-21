interface ProgressBarProps {
  value: number;
  tone?: "primary" | "success" | "danger";
  className?: string;
}

const toneClass = {
  primary: "bg-primary",
  success: "bg-success",
  danger: "bg-danger",
} as const;

export default function ProgressBar({ value, tone = "primary", className = "" }: ProgressBarProps) {
  const clamped = Math.max(0, Math.min(100, value));
  return (
    <div
      className={`h-1.5 w-full overflow-hidden rounded-full bg-line ${className}`}
      role="progressbar"
      aria-valuenow={clamped}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div
        className={`h-full origin-left rounded-full transition-transform duration-200 ease-skillbox ${toneClass[tone]}`}
        style={{ transform: `scaleX(${clamped / 100})` }}
      />
    </div>
  );
}
