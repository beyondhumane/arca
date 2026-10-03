import { createContext, useContext, useEffect } from 'react';
import type { ReactNode } from 'react';

export const LANGS = ['en', 'es'] as const;
export type Lang = (typeof LANGS)[number];

/** Each language named in itself, so a reader can find theirs whatever is showing. */
export const LANG_NAMES: Record<Lang, string> = { en: 'English', es: 'Español' };

const KEY = 'arca-lang';

export function storedLang(): Lang | null {
  try {
    const v = localStorage.getItem(KEY);
    return LANGS.includes(v as Lang) ? (v as Lang) : null;
  } catch {
    return null;
  }
}

export function storeLang(l: Lang) {
  try {
    localStorage.setItem(KEY, l);
  } catch {
    /* private mode: the choice lasts until reload */
  }
}

const LangContext = createContext<{ lang: Lang; setLang: (l: Lang) => void }>({ lang: 'en', setLang: () => {} });

export function LangProvider({ lang, setLang, children }: { lang: Lang; setLang: (l: Lang) => void; children: ReactNode }) {
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
  return <LangContext.Provider value={{ lang, setLang }}>{children}</LangContext.Provider>;
}

export const useLang = () => useContext(LangContext);

/** Picks the copy for the current language. Spanish must mirror the English shape. */
export function useCopy<T>(copy: { en: T; es: NoInfer<T> }): T {
  return copy[useLang().lang];
}
