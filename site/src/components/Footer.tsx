import { ArrowUp } from 'lucide-react';
import {
  CONTRIBUTORS_URL,
  ISSUES_URL,
  LICENSE_URL,
  PULLS_URL,
  RELEASES_URL,
  REPO_URL,
  VERSION,
} from '@/lib/site';
import { useCopy, useLang } from '@/lib/i18n';
import type { Lang } from '@/lib/i18n';
import { Container, Logo } from './ui';
import { GitHubIcon } from './icons';
import { docPath, homePath } from '@/lib/router';

type Column = { title: string; links: { label: string; href: string; external?: boolean }[] };

const columns = (lang: Lang, l: string[][]): Column[] => [
  {
    title: l[0][0],
    links: [
      { label: l[0][1], href: homePath(lang, 'features') },
      { label: l[0][2], href: homePath(lang, 'showcase') },
      { label: l[0][3], href: homePath(lang, 'performance') },
      { label: l[0][4], href: homePath(lang, 'download') },
    ],
  },
  {
    title: l[1][0],
    links: [
      { label: l[1][1], href: docPath(lang, 'introduction') },
      { label: l[1][2], href: docPath(lang, 'installation') },
      { label: l[1][3], href: docPath(lang, 'cli-reference') },
      { label: l[1][4], href: docPath(lang, 'encryption') },
      { label: l[1][5], href: docPath(lang, 'benchmarks') },
    ],
  },
  {
    title: l[2][0],
    links: [
      { label: l[2][1], href: REPO_URL, external: true },
      { label: l[2][2], href: ISSUES_URL, external: true },
      { label: l[2][3], href: PULLS_URL, external: true },
      { label: l[2][4], href: RELEASES_URL, external: true },
      { label: l[2][5], href: CONTRIBUTORS_URL, external: true },
    ],
  },
];

const COPY = {
  en: {
    columns: columns('en', [
      ['Product', 'Features', 'Product tour', 'Performance', 'Download'],
      ['Documentation', 'Introduction', 'Installation', 'CLI reference', 'Encryption', 'Benchmarks'],
      ['Community', 'Repository', 'Issues', 'Pull requests', 'Releases', 'Contributors'],
    ]),
    top: 'Arca, back to top',
    tagline: 'Open-source archiver for Windows, macOS and Linux, written in Rust. Every benchmark ships with its command.',
    github: 'Arca on GitHub',
    stable: 'stable',
    released: 'Released under the',
    notAffiliated: 'Not affiliated with 7-Zip, WinRAR or NanaZip.',
    privacy: 'This site sets no cookies and loads nothing from third parties.',
    backToTop: 'Back to top',
  },
  es: {
    columns: columns('es', [
      ['Producto', 'Funciones', 'Recorrido', 'Rendimiento', 'Descargar'],
      ['Documentación', 'Introducción', 'Instalación', 'Referencia de la CLI', 'Cifrado', 'Benchmarks'],
      ['Comunidad', 'Repositorio', 'Issues', 'Pull requests', 'Versiones', 'Colaboradores'],
    ]),
    top: 'Arca, volver arriba',
    tagline: 'Archivador de código abierto para Windows, macOS y Linux, escrito en Rust. Cada benchmark viene con su comando.',
    github: 'Arca en GitHub',
    stable: 'estable',
    released: 'Publicado bajo la',
    notAffiliated: 'Sin relación con 7-Zip, WinRAR ni NanaZip.',
    privacy: 'Este sitio no usa cookies ni carga nada de terceros.',
    backToTop: 'Volver arriba',
  },
};

export function Footer() {
  const t = useCopy(COPY);
  const { lang } = useLang();
  return (
    <footer className="relative overflow-hidden border-t border-white/[0.06] pt-16 sm:pt-20">
      <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 top-0 h-px bg-linear-to-r from-transparent via-brand-400/40 to-transparent" />
      <Container>
        <div className="grid grid-cols-2 gap-x-8 gap-y-12 sm:grid-cols-3 lg:grid-cols-[minmax(0,1.3fr)_repeat(3,minmax(0,1fr))]">
          <div className="col-span-2 sm:col-span-3 lg:col-span-1">
            <a href={homePath(lang, 'top')} aria-label={t.top} className="inline-block rounded-lg">
              <Logo />
            </a>
            <p className="mt-5 max-w-xs text-sm leading-relaxed text-slate-400">
              {t.tagline}
            </p>
            <div className="mt-6 flex items-center gap-3">
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer noopener"
                aria-label={t.github}
                className="flex size-10 items-center justify-center rounded-xl border border-white/10 bg-white/[0.03] text-slate-400 transition hover:-translate-y-0.5 hover:border-white/20 hover:text-white"
              >
                <GitHubIcon className="size-[18px]" />
              </a>
              <span className="inline-flex items-center gap-2 rounded-full border border-white/10 px-3 py-1.5 text-xs text-slate-400">
                <span className="size-1.5 animate-pulse-dot rounded-full bg-emerald-400" />
                v{VERSION} · {t.stable}
              </span>
            </div>
          </div>

          {t.columns.map((col) => (
            <nav key={col.title} aria-label={col.title}>
              <h2 className="text-sm font-medium text-white">{col.title}</h2>
              <ul className="mt-4 space-y-3">
                {col.links.map((l) => (
                  <li key={l.label}>
                    <a
                      href={l.href}
                      {...(l.external ? { target: '_blank', rel: 'noreferrer noopener' } : {})}
                      className="text-sm text-slate-400 transition-colors hover:text-white"
                    >
                      {l.label}
                    </a>
                  </li>
                ))}
              </ul>
            </nav>
          ))}
        </div>

        <div className="mt-16 flex flex-col-reverse items-start justify-between gap-4 border-t border-white/[0.06] py-8 text-xs text-slate-400 sm:flex-row sm:items-center">
          <p>
            © {new Date().getFullYear()} Proyecto Arca. {t.released}{' '}
            <a href={LICENSE_URL} target="_blank" rel="noreferrer noopener" className="text-slate-300 underline decoration-white/20 underline-offset-4 hover:text-white">
              Apache License 2.0
            </a>
            . {t.notAffiliated} {t.privacy}
          </p>
          <a href={homePath(lang, 'top')} className="group inline-flex items-center gap-2 text-slate-400 transition hover:text-white">
            {t.backToTop}
            <span className="flex size-7 items-center justify-center rounded-full border border-white/10 transition group-hover:-translate-y-0.5 group-hover:border-white/25">
              <ArrowUp className="size-3.5" />
            </span>
          </a>
        </div>
      </Container>

      <div aria-hidden="true" className="pointer-events-none select-none overflow-hidden">
        <div className="fade-b -mb-[4vw] text-center text-[26vw] font-semibold leading-[0.8] tracking-[-0.07em] text-transparent [-webkit-text-stroke:1px_rgba(255,255,255,0.07)] sm:-mb-[3vw]">
          arca
        </div>
      </div>
    </footer>
  );
}
