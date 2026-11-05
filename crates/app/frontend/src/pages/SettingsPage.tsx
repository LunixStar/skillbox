/// 设置页：界面语言（zh-CN / en，立即生效并持久化）+ 中央库入口。
/// 语言持久化由 I18nProvider 的 setLang 统一落 settings 表，此处只负责 UI。

import type { CSSProperties } from "react";
import { FolderOpen } from "lucide-react";
import { ipcError } from "../api/ipc";
import { useOpenPath } from "../api/queries";
import { useToast } from "../components/Toast";
import { Button } from "../components/ui";
import { useI18n } from "../i18n";

const LANG_OPTIONS: { value: "zh-CN" | "en"; label: string }[] = [
  { value: "zh-CN", label: "简体中文" },
  { value: "en", label: "English" },
];

export default function SettingsPage() {
  const { lang, setLang, t } = useI18n();
  const openPath = useOpenPath();
  const toast = useToast();

  return (
    <div className="flex flex-1 flex-col px-8 py-6">
      <h1 className="text-lg font-semibold text-ink">{t("settings.title")}</h1>
      <p className="mt-1 text-xs text-ink-secondary">{t("settings.subtitle")}</p>

      <div className="mt-5 max-w-xl space-y-3">
        {/* 界面语言（§0.4 Phase 5 i18n） */}
        <section className="sb-card sb-fade-up p-4" style={{ "--sb-i": 0 } as CSSProperties}>
          <h3 className="text-sm font-semibold text-ink">{t("settings.language")}</h3>
          <p className="mt-1 text-xs leading-5 text-ink-secondary">{t("settings.languageHint")}</p>
          <div className="mt-3 flex items-center gap-0.5 rounded-btn border border-line bg-page p-0.5">
            {LANG_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                type="button"
                className={`h-8 rounded-[6px] px-4 text-sm transition-colors duration-120 ${
                  lang === opt.value
                    ? "bg-primary-soft font-medium text-primary-active"
                    : "text-ink-secondary hover:bg-surface"
                }`}
                onClick={() => setLang(opt.value)}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </section>

        {/* 中央库（只读展示 + 打开入口，路径由后端管理 §3.1） */}
        <section className="sb-card sb-fade-up p-4" style={{ "--sb-i": 1 } as CSSProperties}>
          <h3 className="text-sm font-semibold text-ink">{t("settings.centralTitle")}</h3>
          <p className="mt-1 font-mono text-xs leading-5 text-ink-secondary">~/.skillbox/skills</p>
          <p className="mt-1 text-xs leading-5 text-ink-secondary">{t("settings.centralDesc")}</p>
          <div className="mt-3">
            <Button
              variant="secondary"
              size="sm"
              disabled={openPath.isPending}
              onClick={() => openPath.mutate("central", { onError: (e) => toast.show(ipcError(e), "danger") })}
            >
              <FolderOpen size={13} />
              {t("settings.openCentral")}
            </Button>
          </div>
        </section>
      </div>
    </div>
  );
}
