interface SkeletonProps {
  className?: string;
  circle?: boolean;
}

export default function Skeleton({ className = "", circle = false }: SkeletonProps) {
  return (
    <div
      aria-hidden
      className={`sb-pulse bg-line ${circle ? "rounded-full" : "rounded-[6px]"} ${className}`}
    />
  );
}
