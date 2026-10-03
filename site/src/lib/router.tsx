import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { DOCS } from '@/components/docs/registry';
import { LANGS, storedLang } from '@/lib/i18n';
import type { Lang } from '@/lib/i18n';

export type Route =
  | { lang: Lang; view: 'home'; anchor: string }
  | { lang: Lang; view: 'docs'; slug: string; section?: string }
  | { lang: Lang; view: 'missing' };

const prefix = (lang: Lang) => (lang === 'en' ? '' : `/${lang}`);

export const homePath = (lang: Lang, anchor?: string) => `${prefix(lang)}/${anchor ? `#${anchor}` : ''}`;

export const docPath = (lang: Lang, slug = 'introduction', section?: string) =>
  `${prefix(lang)}/docs/${slug}/${section ? `#${section}` : ''}`;

export function routePath(route: Route, lang: Lang = route.lang): string {
  if (route.view === 'docs') return docPath(lang, route.slug, route.section);
  if (route.view === 'home') return homePath(lang, route.anchor);
  return homePath(lang);
}

export function parseRoute(pathname: string, hash = ''): Route {
  const parts = pathname.replace(/index\.html$/, '').split('/').filter(Boolean);
  const lang: Lang = LANGS.includes(parts[0] as Lang) && parts[0] !== 'en' ? (parts.shift() as Lang) : 'en';
  const anchor = decodeURIComponent(hash.replace(/^#/, ''));
  if (parts.length === 0) return { lang, view: 'home', anchor };
  if (parts[0] === 'docs' && parts.length <= 2) {
    const slug = parts[1] ?? 'introduction';
    if (DOCS.en.some((p) => p.slug === slug)) return { lang, view: 'docs', slug, section: anchor || undefined };
  }
  return { lang, view: 'missing' };
}

export function initialLocation(): string {
  const { pathname, hash } = window.location;
  let path = pathname + hash;
  const route = parseRoute(pathname);
  if (route.view === 'home' && hash.startsWith('#/')) {
    const h = hash.slice(2).replace(/\/+$/, '');
    if (h === 'docs' || h.startsWith('docs/')) {
      const [slug, section] = h.slice('docs'.length).replace(/^\/+/, '').split('/');
      path = docPath(route.lang, slug || undefined, section);
    } else {
      path = homePath(route.lang, h);
    }
  }
  const saved = storedLang();
  const parsed = parseRoute(path.split('#')[0], path.includes('#') ? path.slice(path.indexOf('#')) : '');
  if (saved && saved !== parsed.lang) path = routePath(parsed, saved);
  if (path !== pathname + hash) window.history.replaceState(null, '', path);
  return path;
}

type Location = { pathname: string; hash: string; n: number };

const RouterContext = createContext<{ route: Route; navigate: (href: string) => void }>({
  route: { lang: 'en', view: 'home', anchor: '' },
  navigate: () => {},
});

const split = (url: string, n = 0): Location => {
  const i = url.indexOf('#');
  return i < 0 ? { pathname: url, hash: '', n } : { pathname: url.slice(0, i), hash: url.slice(i), n };
};

export function RouterProvider({ url, children }: { url: string; children: ReactNode }) {
  const [loc, setLoc] = useState<Location>(() => split(url));

  const navigate = useCallback((href: string) => {
    const u = new URL(href, window.location.href);
    if (u.pathname + u.hash !== window.location.pathname + window.location.hash) {
      window.history.pushState(null, '', u.pathname + u.hash);
    }
    setLoc((l) => ({ pathname: u.pathname, hash: u.hash, n: l.n + 1 }));
  }, []);

  useEffect(() => {
    const onPop = () => setLoc((l) => ({ pathname: window.location.pathname, hash: window.location.hash, n: l.n + 1 }));
    const onClick = (e: MouseEvent) => {
      if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      const a = (e.target as Element | null)?.closest?.('a');
      if (!a || (a.target && a.target !== '_self') || a.hasAttribute('download')) return;
      const u = new URL(a.href, window.location.href);
      if (u.origin !== window.location.origin || /\.[a-z0-9]+$/i.test(u.pathname)) return;
      e.preventDefault();
      navigate(u.pathname + u.hash);
    };
    window.addEventListener('popstate', onPop);
    document.addEventListener('click', onClick);
    return () => {
      window.removeEventListener('popstate', onPop);
      document.removeEventListener('click', onClick);
    };
  }, [navigate]);

  const route = useMemo(() => parseRoute(loc.pathname, loc.hash), [loc]);
  const value = useMemo(() => ({ route, navigate }), [route, navigate]);
  return <RouterContext.Provider value={value}>{children}</RouterContext.Provider>;
}

export const useRouter = () => useContext(RouterContext);
export const useRoute = () => useContext(RouterContext).route;
