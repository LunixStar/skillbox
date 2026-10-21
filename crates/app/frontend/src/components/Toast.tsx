/// 轻量 toast（自建，约 60 行）：mutation / IPC 错误的统一出口。
/// 右下角堆叠，进入 sb-slide-right-enter，2.6s 后 sb-slide-right-exit 移除。

import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
  type ReactNode,
} from "react";

type ToastTone = "info" | "danger" | "success";

interface ToastItem {
  id: number;
  tone: ToastTone;
  message: string;
  leaving: boolean;
}

interface ToastApi {
  show: (message: string, tone?: ToastTone) => void;
}

const ToastContext = createContext<ToastApi>({ show: () => {} });

export function useToast() {
  return useContext(ToastContext);
}

const toneClass: Record<ToastTone, string> = {
  info: "border-line bg-page text-ink",
  danger: "border-[rgba(214,69,69,0.35)] bg-page text-danger",
  success: "border-[rgba(46,160,98,0.35)] bg-page text-success",
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const nextId = useRef(1);

  const remove = useCallback((id: number) => {
    setItems((prev) => prev.map((t) => (t.id === id ? { ...t, leaving: true } : t)));
    window.setTimeout(() => {
      setItems((prev) => prev.filter((t) => t.id !== id));
    }, 220);
  }, []);

  const show = useCallback(
    (message: string, tone: ToastTone = "info") => {
      const id = nextId.current++;
      setItems((prev) => [...prev.slice(-4), { id, tone, message, leaving: false }]);
      window.setTimeout(() => remove(id), 2600);
    },
    [remove],
  );

  return (
    <ToastContext.Provider value={{ show }}>
      {children}
      <div className="pointer-events-none fixed right-4 bottom-4 z-[80] flex w-80 flex-col gap-2">
        {items.map((t) => (
          <div
            key={t.id}
            className={`pointer-events-auto rounded-btn border px-3.5 py-2.5 text-xs leading-5 shadow-[0_6px_24px_rgba(16,40,80,0.14)] ${toneClass[t.tone]} ${
              t.leaving ? "sb-slide-right-exit" : "sb-slide-right-enter"
            }`}
            role="status"
          >
            {t.message}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}
