import { useEffect, useRef, useState } from 'react';
import type { FocusEvent as ReactFocusEvent, KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from 'react';
import { ArrowRight, BookOpen, Check, ChevronDown, Languages, Menu, Star, X } from 'lucide-react';
import { cn } from '@/utils/cn';
import { formatCount, useGitHubStars, useScrolled, useScrollSpy } from '@/lib/hooks';
import { RELEASE_URL, REPO_URL, VERSION } from '@/lib/site';
import { LANG_NAMES, LANGS, useCopy, useLang } from '@/lib/i18n';
import type { Lang } from '@/lib/i18n';
import { ButtonLink, Logo } from './ui';
import { GitHubIcon } from './icons';

const SPY_IDS = ['features', 'showcase', 'performance', 'faq'];

const COPY = {
  en: {
    links: ['Features', 'Product', 'Performance', 'FAQ'],
    docs: 'Docs',
    documentation: 'Documentation',
    main: 'Main',
    home: 'Arca, home',
    release: (v: string) => `Release notes for version ${v}`,
    star: 'Star',
    starLabel: (n: number | null) => (n === null ? 'Star Arca on GitHub' : `Star Arca on GitHub, ${n} stars`),
    download: 'Download',
    openMenu: 'Open menu',
    closeMenu: 'Close menu',
    language: 'Language',
  },
  es: {
    links: ['Funciones', 'Producto', 'Rendimiento', 'Preguntas'],
    docs: 'Docs',
    documentation: 'Documentación',
    main: 'Principal',
    home: 'Arca, inicio',
    release: (v: string) => `Notas de la versión ${v}`,
    star: 'Star',
    starLabel: (n: number | null) => (n === null ? 'Dar una estrella a Arca en GitHub' : `Dar una estrella a Arca en GitHub, ${n} estrellas`),
    download: 'Descargar',
    openMenu: 'Abrir menú',
    closeMenu: 'Cerrar menú',
    language: 'Idioma',
  },
};

function LangMenu({ className }: { className?: string }) {
  const { lang, setLang } = useLang();
  const t = useCopy(COPY);
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const items = () => Array.from(root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]') ?? []);

  useEffect(() => {
    if (!open) return;
    items().find((b) => b.getAttribute('aria-checked') === 'true')?.focus();
    const onDown = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      setOpen(false);
      root.current?.querySelector<HTMLButtonElement>('[aria-haspopup]')?.focus();
    };
    document.addEventListener('pointerdown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const onMenuKey = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const list = items();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    list[(i + (e.key === 'ArrowDown' ? 1 : list.length - 1)) % list.length]?.focus();
  };

  const choose = (l: Lang) => {
    setLang(l);
    setOpen(false);
    root.current?.querySelector<HTMLButtonElement>('[aria-haspopup]')?.focus();
  };

  return (
    <div ref={root} className={cn('relative', className)}>
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`${t.language}: ${LANG_NAMES[lang]}`}
        onClick={() => setOpen((o) => !o)}
        className="inline-flex h-9 items-center gap-1.5 rounded-full border border-white/10 bg-white/[0.03] px-3 font-mono text-[12px] uppercase text-slate-300 transition hover:border-white/20 hover:bg-white/[0.06] hover:text-white"
      >
        <Languages className="size-3.5" />
        {lang}
        <ChevronDown className={cn('size-3 text-slate-500 transition-transform duration-300', open && 'rotate-180')} />
      </button>
      <div
        role="menu"
        aria-label={t.language}
        onKeyDown={onMenuKey}
        className={cn(
          'glass-dark absolute right-0 top-full mt-2 min-w-44 origin-top-right rounded-xl p-1.5 shadow-[0_24px_60px_-20px_rgba(0,0,0,0.8)] transition-all duration-200',
          open ? 'visible scale-100 opacity-100' : 'invisible scale-95 opacity-0',
        )}
      >
        {LANGS.map((l) => (
          <button
            key={l}
            type="button"
            role="menuitemradio"
            aria-checked={l === lang}
            lang={l}
            tabIndex={open ? 0 : -1}
            onClick={() => choose(l)}
            className={cn(
              'flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors focus:outline-none focus-visible:bg-white/[0.07]',
              l === lang ? 'text-white' : 'text-slate-400 hover:bg-white/[0.06] hover:text-white',
            )}
          >
            <span className="flex-1">{LANG_NAMES[l]}</span>
            <span className="font-mono text-[11px] uppercase text-slate-500">{l}</span>
            <Check className={cn('size-3.5 text-brand-300', l !== lang && 'invisible')} />
          </button>
        ))}
      </div>
    </div>
  );
}

export function Navbar({ view }: { view: 'home' | 'docs' }) {
  const scrolled = useScrolled(16);
  const active = useScrollSpy(SPY_IDS, view === 'home');
  const stars = useGitHubStars();
  const t = useCopy(COPY);
  const LINKS = SPY_IDS.map((id, i) => ({ id, label: t.links[i] }));
  const [open, setOpen] = useState(false);
  const listRef = useRef<HTMLUListElement>(null);
  const [pill, setPill] = useState({ left: 0, width: 0, visible: false });

  useEffect(() => {
    setOpen(false);
  }, [view]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const onResize = () => {
      if (window.innerWidth >= 1024) setOpen(false);
    };
    document.addEventListener('keydown', onKey);
    window.addEventListener('resize', onResize);
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      window.removeEventListener('resize', onResize);
      document.body.style.overflow = prevOverflow;
    };
  }, [open]);

  const movePill = (e: ReactMouseEvent<HTMLAnchorElement> | ReactFocusEvent<HTMLAnchorElement>) => {
    const parent = listRef.current;
    if (!parent) return;
    const r = e.currentTarget.getBoundingClientRect();
    const pr = parent.getBoundingClientRect();
    setPill({ left: r.left - pr.left, width: r.width, visible: true });
  };
  const hidePill = () => setPill((p) => ({ ...p, visible: false }));

  const solid = scrolled || open || view === 'docs';
  const showStars = stars !== null && stars >= 10;

  const mobileLinks = [
    ...LINKS.map((l) => ({ href: `#${l.id}`, label: l.label })),
    { href: '#/docs', label: t.documentation },
  ];

  return (
    <header className="pointer-events-none fixed inset-x-0 top-0 z-50">
      <div
        className={cn(
          'mx-auto max-w-7xl px-3 transition-[padding] duration-500 sm:px-6 lg:px-8',
          solid ? 'pt-2.5' : 'pt-4 sm:pt-5',
        )}
      >
        <nav
          aria-label={t.main}
          className={cn(
            'pointer-events-auto relative flex h-14 items-center justify-between gap-4 rounded-2xl border pl-3 pr-2 transition-all duration-500 sm:pl-4',
            solid
              ? 'glass-dark border-white/10 shadow-[0_12px_40px_-12px_rgba(0,0,0,0.7)]'
              : 'border-transparent bg-transparent',
          )}
        >
          <div className="flex items-center gap-3">
            <a href="#top" aria-label={t.home} className="rounded-lg">
              <Logo />
            </a>
            <a
              href={RELEASE_URL}
              target="_blank"
              rel="noreferrer noopener"
              className="hidden rounded-full border border-white/10 bg-white/[0.03] px-2 py-0.5 font-mono text-[11px] text-slate-400 transition hover:border-brand-400/40 hover:text-brand-200 sm:inline-flex"
              aria-label={t.release(VERSION)}
            >
              v{VERSION}
            </a>
          </div>

          <ul ref={listRef} onMouseLeave={hidePill} className="relative hidden items-center lg:flex">
            <span
              aria-hidden="true"
              className="pointer-events-none absolute inset-y-0 rounded-full bg-white/[0.07] transition-all duration-300 ease-out"
              style={{ left: pill.left, width: pill.width, opacity: pill.visible ? 1 : 0 }}
            />
            {LINKS.map((l) => {
              const isActive = view === 'home' && active === l.id;
              return (
                <li key={l.id}>
                  <a
                    href={`#${l.id}`}
                    onMouseEnter={movePill}
                    onFocus={movePill}
                    onBlur={hidePill}
                    aria-current={isActive ? 'true' : undefined}
                    className={cn(
                      'relative block rounded-full px-3.5 py-2 text-sm transition-colors duration-300',
                      isActive ? 'text-white' : 'text-slate-400 hover:text-white',
                    )}
                  >
                    {l.label}
                    <span
                      aria-hidden="true"
                      className={cn(
                        'absolute inset-x-3 -bottom-[3px] h-px bg-linear-to-r from-transparent via-brand-400 to-transparent transition-opacity duration-500',
                        isActive ? 'opacity-100' : 'opacity-0',
                      )}
                    />
                  </a>
                </li>
              );
            })}
            <li>
              <a
                href="#/docs"
                onMouseEnter={movePill}
                onFocus={movePill}
                onBlur={hidePill}
                aria-current={view === 'docs' ? 'page' : undefined}
                className={cn(
                  'relative flex items-center gap-1.5 rounded-full px-3.5 py-2 text-sm transition-colors duration-300',
                  view === 'docs' ? 'text-white' : 'text-slate-400 hover:text-white',
                )}
              >
                <BookOpen className="size-3.5" />
                {t.docs}
              </a>
            </li>
          </ul>

          <div className="flex items-center gap-2">
            <a
              href={REPO_URL}
              target="_blank"
              rel="noreferrer noopener"
              className="group hidden h-9 items-center gap-2 rounded-full border border-white/10 bg-white/[0.03] px-3 text-[13px] text-slate-300 transition hover:border-white/20 hover:bg-white/[0.06] hover:text-white sm:inline-flex"
              aria-label={t.starLabel(showStars ? stars : null)}
            >
              <GitHubIcon className="size-4" />
              <span>{t.star}</span>
              {showStars && stars !== null && (
                <span className="rounded-full bg-white/[0.08] px-1.5 font-mono text-[11px] text-slate-200">
                  {formatCount(stars)}
                </span>
              )}
              <Star className="size-3.5 text-slate-500 transition-all duration-500 group-hover:rotate-[72deg] group-hover:fill-brand-400 group-hover:text-brand-400" />
            </a>
            <LangMenu />
            <ButtonLink href="#download" size="sm" className="hidden sm:inline-flex">
              {t.download}
            </ButtonLink>
            <button
              type="button"
              className="inline-flex size-10 items-center justify-center rounded-xl text-slate-300 transition hover:bg-white/[0.06] hover:text-white lg:hidden"
              aria-expanded={open}
              aria-controls="mobile-nav"
              aria-label={open ? t.closeMenu : t.openMenu}
              onClick={() => setOpen((o) => !o)}
            >
              <span className="relative size-5">
                <Menu
                  className={cn(
                    'absolute inset-0 size-5 transition-all duration-300',
                    open ? 'rotate-90 scale-50 opacity-0' : 'rotate-0 scale-100 opacity-100',
                  )}
                />
                <X
                  className={cn(
                    'absolute inset-0 size-5 transition-all duration-300',
                    open ? 'rotate-0 scale-100 opacity-100' : '-rotate-90 scale-50 opacity-0',
                  )}
                />
              </span>
            </button>
          </div>
        </nav>

        <div
          id="mobile-nav"
          className={cn(
            'glass-dark pointer-events-auto mt-2 origin-top overflow-hidden rounded-2xl p-2 shadow-[0_24px_60px_-20px_rgba(0,0,0,0.8)] transition-all duration-500 ease-out lg:hidden',
            open ? 'visible translate-y-0 scale-100 opacity-100' : 'invisible -translate-y-2 scale-[0.98] opacity-0',
          )}
        >
          <ul className="flex flex-col">
            {mobileLinks.map((l, i) => (
              <li
                key={l.href}
                className={cn(
                  'transition-all duration-500 ease-out',
                  open ? 'translate-y-0 opacity-100' : '-translate-y-1 opacity-0',
                )}
                style={{ transitionDelay: open ? `${60 + i * 45}ms` : '0ms' }}
              >
                <a
                  href={l.href}
                  onClick={() => setOpen(false)}
                  className="flex items-center justify-between rounded-xl px-4 py-3.5 text-[15px] text-slate-200 transition hover:bg-white/[0.05] hover:text-white"
                >
                  {l.label}
                  <ArrowRight className="size-4 text-slate-500" />
                </a>
              </li>
            ))}
          </ul>
          <div className="mt-1 grid grid-cols-2 gap-2 border-t border-white/[0.06] p-2 pt-3">
            <ButtonLink href={REPO_URL} external variant="glass" size="md">
              <GitHubIcon className="size-4" />
              GitHub
            </ButtonLink>
            <ButtonLink href="#download" size="md" onClick={() => setOpen(false)}>
              {t.download}
            </ButtonLink>
          </div>
        </div>
      </div>

      {open && (
        <div
          aria-hidden="true"
          className="pointer-events-auto fixed inset-0 -z-10 bg-ink-950/60 backdrop-blur-sm lg:hidden"
          onClick={() => setOpen(false)}
        />
      )}
    </header>
  );
}
