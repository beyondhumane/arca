import { ArrowRight, BookOpen, Download } from 'lucide-react';
import { cn } from '@/utils/cn';
import { CLONE_CMD, DOWNLOADS, RELEASE_URL, VERSION } from '@/lib/site';
import type { Platform } from '@/lib/site';
import { Accent, ButtonLink, Container, CopyButton, Reveal } from '../ui';
import { DownloadButton, PLATFORM_ICONS, useRecommendedDownload } from '../download';
import { useCopy } from '@/lib/i18n';

const COPY = {
  en: {
    title: ['Your next archive is ', 'one command', ' away.'],
    lead: 'Download a prebuilt binary for your platform, or build it from source with one command.',
    docs: 'Read the docs',
    download: (f: string) => `Download ${f}`,
    yours: 'Your system',
    details: { windows: 'Installer · x86_64', macos: 'Apple silicon · arm64', 'macos-intel': 'Intel · x86_64', linux: 'x86_64' },
    copyBuild: 'Copy build-from-source command',
    rust: 'Needs Rust 1.95+.',
    notes: 'Release notes & checksums',
  },
  es: {
    title: ['Tu próximo archivo está a ', 'un comando', '.'],
    lead: 'Descarga un binario para tu plataforma o compílalo desde el código fuente con un solo comando.',
    docs: 'Leer la documentación',
    download: (f: string) => `Descargar ${f}`,
    yours: 'Tu sistema',
    details: { windows: 'Instalador · x86_64', macos: 'Apple silicon · arm64', 'macos-intel': 'Intel · x86_64', linux: 'x86_64' },
    copyBuild: 'Copiar el comando para compilar',
    rust: 'Requiere Rust 1.95 o superior.',
    notes: 'Notas de la versión y sumas',
  },
};

const ORDER: Platform[] = ['windows', 'macos', 'macos-intel', 'linux'];

export function CTA() {
  const { platform: recommended } = useRecommendedDownload();
  const t = useCopy(COPY);

  return (
    <section id="download" aria-labelledby="cta-title" className="relative py-24 sm:py-32">
      <Container>
        <Reveal variant="scale">
          <div className="beam-border relative overflow-hidden rounded-[36px] bg-ink-900 px-6 py-16 sm:px-12 sm:py-20 lg:px-16">
            <div aria-hidden="true" className="pointer-events-none absolute inset-0">
              <div className="pattern-grid fade-radial absolute inset-0 opacity-50" />
              <div className="absolute -top-40 left-1/2 h-[520px] w-[900px] -translate-x-1/2 animate-aurora rounded-full bg-[radial-gradient(closest-side,rgba(47,107,255,0.3),transparent_70%)]" />
              <div className="absolute -bottom-48 right-[-10%] h-[420px] w-[620px] animate-aurora-slow rounded-full bg-[radial-gradient(closest-side,rgba(255,138,61,0.2),transparent_70%)]" />
            </div>

            <div className="relative mx-auto max-w-3xl text-center">
              <p className="font-mono text-xs uppercase tracking-[0.2em] text-brand-300">v{VERSION} · Apache-2.0</p>
              <h2 id="cta-title" className="mt-5 text-balance text-4xl font-semibold leading-[1.04] tracking-[-0.04em] text-white sm:text-6xl">
                {t.title[0]}<Accent>{t.title[1]}</Accent>{t.title[2]}
              </h2>
              <p className="mx-auto mt-5 max-w-xl text-pretty text-base leading-relaxed text-slate-400 sm:text-lg">
                {t.lead}
              </p>
              <div className="mt-9 flex flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center">
                <DownloadButton />
                <ButtonLink href="#/docs" variant="glass" size="lg">
                  <BookOpen className="size-[18px]" />
                  {t.docs}
                </ButtonLink>
              </div>
            </div>

            <div className="relative mx-auto mt-14 grid max-w-5xl gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {ORDER.map((p) => {
                const d = DOWNLOADS[p];
                const Icon = PLATFORM_ICONS[p];
                const isRec = recommended === p;
                return (
                  <a
                    key={p}
                    href={d.url}
                    aria-label={t.download(d.file)}
                    className={cn(
                      'group relative flex items-center gap-4 rounded-2xl border p-4 text-left transition-all duration-300 hover:-translate-y-1',
                      isRec
                        ? 'border-brand-400/40 bg-brand-500/[0.08] shadow-[0_20px_50px_-24px_rgba(47,107,255,0.7)]'
                        : 'border-white/[0.08] bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.05]',
                    )}
                  >
                    {isRec && (
                      <span className="absolute -top-2.5 right-3 rounded-full bg-brand-400 px-2 py-0.5 text-[10px] font-semibold text-ink-950">
                        {t.yours}
                      </span>
                    )}
                    <span className="flex size-11 shrink-0 items-center justify-center rounded-xl border border-white/10 bg-black/30 text-slate-100 transition-transform duration-300 group-hover:scale-110">
                      <Icon className="size-5" />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block text-sm font-medium text-white">{d.label}</span>
                      <span className="block truncate text-xs text-slate-500">{[t.details[p], d.size].filter(Boolean).join(' · ')}</span>
                    </span>
                    <Download className="size-4 shrink-0 text-slate-500 transition-all duration-300 group-hover:translate-y-0.5 group-hover:text-brand-300" />
                  </a>
                );
              })}
            </div>

            <div className="relative mx-auto mt-6 max-w-3xl">
              <div className="flex items-center gap-2 rounded-2xl border border-white/[0.08] bg-black/40 py-2 pl-4 pr-2 backdrop-blur">
                <span className="select-none font-mono text-sm text-brand-400">$</span>
                <code className="no-scrollbar min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[12.5px] text-slate-300 sm:text-[13px]">
                  <span className="text-sky-300">git</span> clone https://github.com/beyondhumane/arca <span className="text-slate-500">&amp;&amp;</span>{' '}
                  <span className="text-sky-300">cd</span> arca <span className="text-slate-500">&amp;&amp;</span>{' '}
                  <span className="text-sky-300">cargo</span> build <span className="text-violet-300">--release</span>
                </code>
                <CopyButton text={CLONE_CMD} label={t.copyBuild} />
              </div>
              <p className="mt-4 text-center text-xs text-slate-500">
                {t.rust}{' '}
                <a href={RELEASE_URL} target="_blank" rel="noreferrer noopener" className="inline-flex items-center gap-1 text-slate-400 underline decoration-white/20 underline-offset-4 transition hover:text-white">
                  {t.notes} <ArrowRight className="size-3" />
                </a>
              </p>
            </div>
          </div>
        </Reveal>
      </Container>
    </section>
  );
}
