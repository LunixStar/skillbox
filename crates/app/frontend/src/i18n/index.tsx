/// 自研 i18n（§1：禁止 i18next）：t() + LanguageProvider，约 70 行。
/// 两份扁平 JSON（zh-CN / en）；key 双向覆盖由 scripts/check-i18n.mjs 校验。
/// 语言持久化在 settings 表 `language` 键；读取失败回退中文。

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { settingsGet, settingsSet } from "../api/ipc";
import en from "./en.json";
import zhCN from "./zh-CN.json";

export type Lang = "zh-CN" | "en";

const LANG_KEY = "language";

const DICTS: Record<Lang, Record<string, string>> = {
  "zh-CN": zhCN,
  en,
};

export type TParams = Record<string, string | number>;

interface I18nApi {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: string, params?: TParams) => string;
}

const I18nContext = createContext<I18nApi | null>(null);

function lookup(lang: Lang, key: string, params?: TParams): string {
  const raw = DICTS[lang][key] ?? DICTS["zh-CN"][key] ?? key;
  if (!params) {
    return raw;
  }
  return raw.replace(/\{(\w+)\}/g, (m, name: string) =>
    name in params ? String(params[name]) : m,
  );
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>("zh-CN");

  useEffect(() => {
    let cancelled = false;
    settingsGet(LANG_KEY)
      .then((v) => {
        if (!cancelled && (v === "en" || v === "zh-CN")) {
          setLangState(v);
        }
      })
      .catch(() => {
        /* 设置表不可用则维持默认语言 */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const setLang = useCallback((next: Lang) => {
    setLangState(next);
    document.documentElement.lang = next;
    settingsSet({ key: LANG_KEY, value: next, dryRun: false }).catch(() => {
      /* 持久化失败不阻断切换（会话内仍生效） */
    });
  }, []);

  const t = useCallback((key: string, params?: TParams) => lookup(lang, key, params), [lang]);

  const api = useMemo<I18nApi>(() => ({ lang, setLang, t }), [lang, setLang, t]);

  return <I18nContext.Provider value={api}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nApi {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error("useI18n 必须在 I18nProvider 内使用");
  }
  return ctx;
}
