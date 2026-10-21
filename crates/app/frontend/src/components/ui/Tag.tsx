import type { ReactNode } from "react";
import { useI18n } from "../../i18n";

interface TagProps {
  children: ReactNode;
  onRemove?: () => void;
}

export default function Tag({ children, onRemove }: TagProps) {
  const { t } = useI18n();
  return (
    <span className="inline-flex items-center gap-1 rounded-tag bg-primary-soft px-2 py-0.5 text-xs text-primary-active">
      {children}
      {onRemove && (
        <button
          type="button"
          className="rounded-full px-0.5 leading-none hover:text-primary"
          onClick={onRemove}
          aria-label={t("common.removeTag")}
        >
          ×
        </button>
      )}
    </span>
  );
}
