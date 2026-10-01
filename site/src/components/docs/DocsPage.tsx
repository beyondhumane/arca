import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  Download,
  ExternalLink,
  Menu,
  Search,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { usePrefersReducedMotion } from '@/lib/hooks';
import { NEW_ISSUE_URL, REPO_URL, VERSION } from '@/lib/site';
import { useCopy, useLang } from '@/lib/i18n';
import { Kbd } from '../ui';
import { DOCS, docGroups } from './registry';
import { Markdown } from './Markdown';

type TocItem = { id: string; text: string; level: 2 | 3 };

const COPY = {
  en: {
    documentation: 'Documentation',
    search: 'Search docs',
    searchLabel: 'Search documentation',
    noMatch: (q: string) => `Nothing matches “${q}”.`,
    clear: 'Clear search',
    get: 'Get Arca',
    breadcrumb: 'Breadcrumb',
    docs: 'Docs',
    prevNext: 'Previous and next pages',
    previous: 'Previous',
    next: 'Next',
    notFound: 'This page doesn’t exist.',
    noPage: 'There’s no docs page called',
    toIntro: 'Go to the introduction',
    onThisPage: 'On this page',
    edit: 'Edit this page',
    report: 'Report an issue',
    top: 'Back to top',
  },
  es: {
    documentation: 'Documentación',
    search: 'Buscar en la guía',
    searchLabel: 'Buscar en la documentación',
    noMatch: (q: string) => `Nada coincide con «${q}».`,
    clear: 'Borrar búsqueda',
    get: 'Descarga Arca',
    breadcrumb: 'Ruta',
    docs: 'Docs',
    prevNext: 'Página anterior y siguiente',
    previous: 'Anterior',
    next: 'Siguiente',
    notFound: 'Esta página no existe.',
    noPage: 'No hay ninguna página llamada',
    toIntro: 'Ir a la introducción',
    onThisPage: 'En esta página',
    edit: 'Editar esta página',
    report: 'Informar de un problema',
    top: 'Volver arriba',
  },
};

export function DocsPage({ slug }: { slug: string }) {
  const { lang } = useLang();
  const t = useCopy(COPY);
  const DOC_PAGES = DOCS[lang];
  const index = DOC_PAGES.findIndex((p) => p.slug === slug);
  const page = index >= 0 ? DOC_PAGES[index] : null;
  const prev = index > 0 ? DOC_PAGES[index - 1] : null;
  const next = index >= 0 && index < DOC_PAGES.length - 1 ? DOC_PAGES[index + 1] : null;

  const reduced = usePrefersReducedMotion();
  const [query, setQuery] = useState('');
  const [navOpen, setNavOpen] = useState(false);
  const [toc, setToc] = useState<TocItem[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const articleRef = useRef<HTMLElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setNavOpen(false);
  }, [slug]);

  /* Table of contents + scroll spy */
  useEffect(() => {
    const root = articleRef.current;
    if (!root) {
      setToc([]);
      return;
    }
    const headings = Array.from(root.querySelectorAll<HTMLElement>('h2[id], h3[id]'));
    setToc(
      headings.map((h) => ({
        id: h.id,
        text: h.textContent ?? '',
        level: h.tagName === 'H3' ? 3 : 2,
      })),
    );
    setActiveId(headings[0]?.id ?? null);
    if (headings.length === 0 || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (visible[0]) setActiveId(visible[0].target.id);
      },
      { rootMargin: '-96px 0px -65% 0px' },
    );
    headings.forEach((h) => io.observe(h));
    return () => io.disconnect();
  }, [slug, lang]);

  /* "/" focuses search */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
      e.preventDefault();
      if (window.innerWidth < 1024) setNavOpen(true);
      window.setTimeout(() => searchRef.current?.focus(), 30);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  const q = query.trim().toLowerCase();
  const groups = useMemo(
    () =>
      docGroups(DOC_PAGES).map((g) => ({
        title: g,
        pages: DOC_PAGES.filter(
          (p) =>
            p.group === g &&
            (!q || p.title.toLowerCase().includes(q) || p.description.toLowerCase().includes(q) || p.keywords.includes(q)),
        ),
      })).filter((g) => g.pages.length > 0),
    [q, DOC_PAGES],
  );

  const scrollToHeading = (id: string) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' });
    setActiveId(id);
  };

  return (
    <div className="relative pb-12 pt-24 sm:pt-28">
      <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-[520px] overflow-hidden">
        <div className="pattern-grid fade-radial absolute inset-0 opacity-50" />
        <div className="absolute left-1/3 top-[-40%] h-[600px] w-[900px] -translate-x-1/2 animate-aurora rounded-full bg-[radial-gradient(closest-side,rgba(47,107,255,0.16),transparent_70%)]" />
      </div>

      <div className="mx-auto max-w-7xl px-5 sm:px-6 lg:grid lg:grid-cols-[250px_minmax(0,1fr)] lg:gap-10 lg:px-8 xl:grid-cols-[250px_minmax(0,1fr)_210px]">
        {/* Mobile toggle */}
        <div className="mb-6 lg:hidden">
          <button
            type="button"
            aria-expanded={navOpen}
            aria-controls="docs-sidebar"
            onClick={() => setNavOpen((o) => !o)}
            className="glass flex w-full items-center justify-between rounded-xl px-4 py-3 text-sm text-slate-200"
          >
            <span className="flex items-center gap-2">
              <Menu className="size-4 text-slate-400" />
              {page ? page.title : t.documentation}
            </span>
            <ChevronDown className={cn('size-4 text-slate-500 transition-transform duration-300', navOpen && 'rotate-180')} />
          </button>
        </div>

        {/* Sidebar */}
        <aside
          id="docs-sidebar"
          className={cn(
            'thin-scrollbar mb-8 lg:sticky lg:top-24 lg:mb-0 lg:block lg:h-[calc(100dvh-7rem)] lg:self-start lg:overflow-y-auto lg:pb-10 lg:pr-2',
            navOpen ? 'block' : 'hidden',
          )}
        >
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-500" />
            <input
              ref={searchRef}
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t.search}
              aria-label={t.searchLabel}
              className="h-10 w-full rounded-xl border border-white/10 bg-white/[0.03] pl-9 pr-10 text-sm text-slate-200 transition placeholder:text-slate-600 focus:border-brand-400/50 focus:bg-white/[0.05] focus:outline-none"
            />
            <Kbd className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2">/</Kbd>
          </div>

          <nav aria-label={t.documentation} className="mt-6 space-y-7">
            {groups.map((g) => (
              <div key={g.title}>
                <h2 className="px-3 font-mono text-[11px] uppercase tracking-[0.16em] text-slate-500">{g.title}</h2>
                <ul className="mt-2 space-y-0.5">
                  {g.pages.map((p) => {
                    const current = p.slug === slug;
                    return (
                      <li key={p.slug}>
                        <a
                          href={`#/docs/${p.slug}`}
                          aria-current={current ? 'page' : undefined}
                          onClick={() => setNavOpen(false)}
                          className={cn(
                            'relative flex items-center rounded-lg px-3 py-2 text-sm transition-colors duration-200',
                            current ? 'bg-brand-500/10 text-white' : 'text-slate-400 hover:bg-white/[0.04] hover:text-slate-100',
                          )}
                        >
                          {current && (
                            <span aria-hidden="true" className="absolute left-0 top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-full bg-brand-400" />
                          )}
                          {p.title}
                        </a>
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))}
            {groups.length === 0 && (
              <p className="px-3 text-sm text-slate-500">
                {t.noMatch(query)}{' '}
                <button type="button" onClick={() => setQuery('')} className="text-brand-300 hover:text-brand-200">
                  {t.clear}
                </button>
              </p>
            )}
          </nav>

          <a
            href="#download"
            className="group mt-8 block rounded-2xl border border-white/[0.08] bg-linear-to-b from-brand-500/[0.09] to-transparent p-4 transition hover:border-brand-400/30"
          >
            <span className="flex items-center gap-2 text-sm font-medium text-white">
              <Download className="size-4 text-brand-300" /> {t.get} v{VERSION}
            </span>
            <span className="mt-1 block text-xs text-slate-500">Windows · macOS · Linux · Apache-2.0</span>
          </a>
        </aside>

        {/* Content */}
        <main id="main" tabIndex={-1} className="min-w-0 outline-none">
          {page ? (
            <>
              <nav aria-label={t.breadcrumb} className="flex flex-wrap items-center gap-1.5 text-xs text-slate-500">
                <a href="#/docs" className="transition hover:text-slate-200">
                  {t.docs}
                </a>
                <ChevronRight className="size-3" />
                <span>{page.group}</span>
                <ChevronRight className="size-3" />
                <span aria-current="page" className="text-slate-300">
                  {page.title}
                </span>
              </nav>

              <article ref={articleRef} key={`${lang}-${page.slug}`} className="animate-fade-up">
                <h1 className="mt-4 text-balance text-4xl font-semibold tracking-[-0.035em] text-white sm:text-5xl">{page.title}</h1>
                <p className="mt-4 max-w-2xl text-pretty text-lg leading-relaxed text-slate-400">{page.description}</p>
                <div className="mt-8 h-px bg-linear-to-r from-white/10 via-white/[0.06] to-transparent" />
                <div className="mt-8 max-w-3xl">
                  <Markdown source={page.body} ids={page.ids} />
                </div>
              </article>

              {/* Prev / next */}
              <nav aria-label={t.prevNext} className="mt-16 grid max-w-3xl gap-3 sm:grid-cols-2">
                {prev ? (
                  <a
                    href={`#/docs/${prev.slug}`}
                    className="group rounded-2xl border border-white/[0.07] p-5 transition-all duration-300 hover:-translate-y-0.5 hover:border-white/[0.15] hover:bg-white/[0.02]"
                  >
                    <span className="flex items-center gap-1.5 text-xs text-slate-500">
                      <ArrowLeft className="size-3.5 transition-transform group-hover:-translate-x-0.5" /> {t.previous}
                    </span>
                    <span className="mt-1 block font-medium text-white transition-colors group-hover:text-brand-200">{prev.title}</span>
                  </a>
                ) : (
                  <span className="hidden sm:block" />
                )}
                {next && (
                  <a
                    href={`#/docs/${next.slug}`}
                    className="group rounded-2xl border border-white/[0.07] p-5 text-right transition-all duration-300 hover:-translate-y-0.5 hover:border-white/[0.15] hover:bg-white/[0.02]"
                  >
                    <span className="flex items-center justify-end gap-1.5 text-xs text-slate-500">
                      {t.next} <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" />
                    </span>
                    <span className="mt-1 block font-medium text-white transition-colors group-hover:text-brand-200">{next.title}</span>
                  </a>
                )}
              </nav>
            </>
          ) : (
            <div className="py-20 text-center">
              <p className="font-mono text-sm text-brand-300">404</p>
              <h1 className="mt-3 text-4xl font-semibold tracking-[-0.035em] text-white">{t.notFound}</h1>
              <p className="mt-4 text-slate-400">
                {t.noPage} <span className="font-mono text-slate-200">{slug}</span>.
              </p>
              <a
                href="#/docs/introduction"
                className="mt-8 inline-flex items-center gap-1.5 rounded-full bg-white px-5 py-2.5 text-sm font-medium text-ink-950 transition hover:-translate-y-0.5"
              >
                {t.toIntro} <ArrowRight className="size-4" />
              </a>
            </div>
          )}
        </main>

        {/* On this page */}
        <aside className="hidden xl:block" aria-label={t.onThisPage}>
          <div className="thin-scrollbar sticky top-24 max-h-[calc(100dvh-7rem)] overflow-y-auto pb-10">
            {toc.length > 0 && (
              <>
                <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-slate-500">{t.onThisPage}</p>
                <ul className="mt-3 border-l border-white/[0.07]">
                  {toc.map((item) => (
                    <li key={item.id}>
                      <button
                        type="button"
                        onClick={() => scrollToHeading(item.id)}
                        className={cn(
                          '-ml-px block w-full border-l py-1.5 text-left text-[13px] leading-snug transition-colors duration-200',
                          item.level === 3 ? 'pl-6' : 'pl-4',
                          activeId === item.id ? 'border-brand-400 text-white' : 'border-transparent text-slate-500 hover:text-slate-200',
                        )}
                      >
                        {item.text}
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
            <div className="mt-8 space-y-2.5 border-t border-white/[0.06] pt-6 text-[13px]">
              {page && (
                <a
                  href={`${REPO_URL}/blob/main/${page.path}`}
                  target="_blank"
                  rel="noreferrer noopener"
                  className="flex items-center gap-1.5 text-slate-500 transition hover:text-white"
                >
                  <ExternalLink className="size-3.5" /> {t.edit}
                </a>
              )}
              <a href={NEW_ISSUE_URL} target="_blank" rel="noreferrer noopener" className="flex items-center gap-1.5 text-slate-500 transition hover:text-white">
                <ExternalLink className="size-3.5" /> {t.report}
              </a>
              <button
                type="button"
                onClick={() => window.scrollTo({ top: 0, behavior: reduced ? 'auto' : 'smooth' })}
                className="flex items-center gap-1.5 text-slate-500 transition hover:text-white"
              >
                <ArrowUp className="size-3.5" /> {t.top}
              </button>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}
