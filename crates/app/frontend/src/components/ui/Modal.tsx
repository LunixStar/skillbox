import { useEffect, useState, type ReactNode } from "react";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  footer?: ReactNode;
  children: ReactNode;
}

export default function Modal({ open, onClose, title, footer, children }: ModalProps) {
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
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className={`absolute inset-0 bg-[rgba(16,40,80,0.32)] backdrop-blur-[8px] ${open ? "sb-fade-enter" : "sb-fade-exit"}`}
        onClick={onClose}
      />
      <div
        className={`relative w-[420px] max-w-[90vw] rounded-card border border-line bg-page shadow-[0_12px_40px_rgba(16,40,80,0.18)] ${open ? "sb-pop-enter" : "sb-pop-exit"}`}
      >
        {title && (
          <div className="border-b border-line px-5 py-3.5 text-sm font-semibold text-ink">
            {title}
          </div>
        )}
        <div className="px-5 py-4 text-sm leading-6 text-ink-secondary">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-line px-5 py-3">{footer}</div>}
      </div>
    </div>
  );
}
