import { useEffect, useState, type ReactNode } from "react";
import { useI18n } from "../../i18n";

interface DrawerProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
}

export default function Drawer({ open, onClose, title, children }: DrawerProps) {
  const { t } = useI18n();
  const [mounted, setMounted] = useState(open);

  useEffect(() => {
    if (open) {
      setMounted(true);
      return;
    }
    const t = setTimeout(() => setMounted(false), 200);
    return () => clearTimeout(t);
  }, [open]);

  if (!mounted) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50">
      <div
        className={`absolute inset-0 bg-[rgba(16,40,80,0.32)] backdrop-blur-[8px] ${open ? "sb-fade-enter" : "sb-fade-exit"}`}
        onClick={onClose}
      />
      <div
        className={`absolute top-0 right-0 flex h-full w-[380px] flex-col border-l border-line bg-page shadow-[0_12px_40px_rgba(16,40,80,0.18)] ${open ? "sb-slide-right-enter" : "sb-slide-right-exit"}`}
      >
        {title && (
          <div className="flex items-center justify-between border-b border-line px-5 py-3.5 text-sm font-semibold text-ink">
            {title}
            <button
              type="button"
              className="rounded-btn px-1.5 text-ink-faint hover:bg-surface hover:text-ink"
              onClick={onClose}
              aria-label={t("common.closeDrawer")}
            >
              ×
            </button>
          </div>
        )}
        <div className="flex-1 overflow-y-auto px-5 py-4 text-sm leading-6 text-ink-secondary">
          {children}
        </div>
      </div>
    </div>
  );
}
