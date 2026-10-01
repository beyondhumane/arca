import { createContext, useContext, useEffect, useState } from 'react';
import type { ReactNode } from 'react';

export const LANGS = ['en', 'es'] as const;
export type Lang = (typeof LANGS)[number];

const KEY = 'arca-lang';

function stored(): Lang | null {
  try {
    const v = localStorage.getItem(KEY);
    return LANGS.includes(v as Lang) ? (v as Lang) : null;
  } catch {
    return null;
  }
}

export function detectLang(): Lang {
  const saved = stored();
  if (saved) return saved;
  const prefs = typeof navigator === 'undefined' ? [] : (navigator.languages ?? [navigator.language]);
  for (const p of prefs) {
    const base = p?.slice(0, 2).toLowerCase();
    if (LANGS.includes(base as Lang)) return base as Lang;
  }
  return 'en';
}

const LangContext = createContext<{ lang: Lang; setLang: (l: Lang) => void }>({ lang: 'en', setLang: () => {} });

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(detectLang);
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
  const setLang = (l: Lang) => {
    setLangState(l);
    try {
      localStorage.setItem(KEY, l);
    } catch {
      /* private mode: the choice lasts until reload */
    }
  };
  return <LangContext.Provider value={{ lang, setLang }}>{children}</LangContext.Provider>;
}

export const useLang = () => useContext(LangContext);

/** Picks the copy for the current language. Spanish must mirror the English shape. */
export function useCopy<T>(copy: { en: T; es: NoInfer<T> }): T {
  return copy[useLang().lang];
}
