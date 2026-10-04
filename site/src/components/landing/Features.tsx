import { useEffect, useState } from 'react';
import type { ComponentType, ReactNode, SVGProps } from 'react';
import { Check, Cpu, FileArchive, Keyboard, Lock, ShieldCheck, Timer, Wrench, Zap } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useCountUp, useInView, usePrefersReducedMotion, useScramble } from '@/lib/hooks';
import { Accent, Container, CopyButton, Kbd, Reveal, SectionHeading, SpotlightCard } from '../ui';
import { useCopy } from '@/lib/i18n';
import { MethodLink } from './MethodLink';

const COPY = {
  en: {
    threadsGroup: 'Number of threads',
    threadLabel: (n: number) => (n === 16 ? 'Every core' : `${n} thread${n > 1 ? 's' : ''}`),
    all: 'all',
    onTwo: 'on 2 threads',
    efficiency: 'scaling efficiency',
    extract: 'to extract 287 MB',
    rejected: 'rejected',
    plain: 'Q3 revenue up 18%, embargoed',
    verified: 'HMAC verified',
    decrypted: (p: string) => `Decrypted contents: ${p}`,
    encrypted: 'Encrypted contents. Activate to decrypt.',
    tapLock: 'Decrypted. Tap again to lock.',
    tapOpen: 'Hover or tap to decrypt',
    zstdCaption: 'Silesia, 212 MB · 120 files · 2 cores · lower is better',
    target: 'design target (R1)',
    readOnly: 'ZipCrypto · read-only',
    shortcuts: ['move', 'open', 'tick', 'tick all', 'back', 'filter'],
    gpu: 'GPU-rendered with GPUI',
    profileGroup: 'Build profile',
    pure: 'pure Rust',
    notes: ['The default. Links libzstd (C) for the fastest Zstandard.', 'No C toolchain needed. Builds for any target Rust supports.'],
    copyBuild: (l: string) => `Copy ${l} build command`,
    eyebrow: 'Features',
    title: ['An archiver built for ', 'speed', ' and hostile input.'],
    description: 'A small binary for the terminal and a native window for everyone else, both on the same core.',
    cards: [
      ['Multi-threaded', 'Every core, on demand', 'Compression and extraction use every core. A .zip indexes its entries, so each thread opens its own entry and shares nothing with the others. Pick a thread count to see it scale.'],
      ['Memory-safe', 'Safe by construction', 'Container parsers forbid unsafe code. Malformed input becomes an error, and Zip Slip paths are refused before a byte is written.'],
      ['Encryption', 'AES-256, authenticated', 'WinZip AE-2 with a fresh salt per entry and an HMAC over every byte. 7-Zip and WinRAR open it, and a tampered entry fails the HMAC check.'],
      ['Codecs', 'Zstandard inside ZIP', 'Ask for zstd and get the fastest archive of the bunch, within 2% of the smallest. Deflate stays the default so any unzip can open it.'],
      ['Startup', 'Starts in under a millisecond', 'Built with LTO and one codegen unit, with no runtime to start up.'],
      ['Formats', 'Speaks the formats you use', 'ZIP with Zip64, 7z with LZMA2 and optional hidden names, plus TAR, gzip and XZ. Read solid 7z archives; create independent blocks. Existing archive editing stays ZIP-only.'],
      ['Desktop', 'A window that respects the keyboard', 'Browse archives like folders, tick with Space, filter with Ctrl+F, and go back with Alt+←. Screen readers get every row.'],
      ['Portable', 'A pure-Rust build', 'Keep native libzstd for peak speed, or flip one flag for a build with no C dependency at all.'],
    ],
  },
  es: {
    threadsGroup: 'Número de hilos',
    threadLabel: (n: number) => (n === 16 ? 'Todos los núcleos' : `${n} hilo${n > 1 ? 's' : ''}`),
    all: 'todos',
    onTwo: 'con 2 hilos',
    efficiency: 'eficiencia de escalado',
    extract: 'para extraer 287 MB',
    rejected: 'rechazada',
    plain: 'Ingresos del T3 +18%, confidencial',
    verified: 'HMAC verificado',
    decrypted: (p: string) => `Contenido descifrado: ${p}`,
    encrypted: 'Contenido cifrado. Actívalo para descifrarlo.',
    tapLock: 'Descifrado. Toca otra vez para cerrarlo.',
    tapOpen: 'Pasa el ratón o toca para descifrar',
    zstdCaption: 'Silesia, 212 MB · 120 archivos · 2 núcleos · menos es mejor',
    target: 'objetivo de diseño (R1)',
    readOnly: 'ZipCrypto · solo lectura',
    shortcuts: ['mover', 'abrir', 'marcar', 'marcar todo', 'atrás', 'filtrar'],
    gpu: 'Dibujado en GPU con GPUI',
    profileGroup: 'Perfil de compilación',
    pure: 'Rust puro',
    notes: ['El predeterminado. Enlaza libzstd (C) para el Zstandard más rápido.', 'Sin toolchain de C. Compila para cualquier destino que admita Rust.'],
    copyBuild: (l: string) => `Copiar el comando de compilación ${l}`,
    eyebrow: 'Funciones',
    title: ['Un archivador hecho para la ', 'velocidad', ' y la entrada hostil.'],
    description: 'Un binario pequeño para la terminal y una ventana nativa para todos los demás, sobre el mismo núcleo.',
    cards: [
      ['Multihilo', 'Todos los núcleos, cuando quieras', 'Comprimir y extraer usan todos los núcleos. Un .zip indexa sus entradas, así que cada hilo abre la suya y no comparte nada con los demás. Elige un número de hilos para ver cómo escala.'],
      ['Seguridad de memoria', 'Seguro por construcción', 'Los analizadores de contenedores prohíben el código unsafe. Una entrada malformada se convierte en un error, y las rutas Zip Slip se rechazan antes de escribir un byte.'],
      ['Cifrado', 'AES-256 autenticado', 'WinZip AE-2 con una sal nueva por entrada y un HMAC sobre cada byte. 7-Zip y WinRAR lo abren, y una entrada manipulada no pasa la comprobación del HMAC.'],
      ['Códecs', 'Zstandard dentro de ZIP', 'Pide zstd y obtienes el archivo más rápido de todos, a menos de un 2% del más pequeño. Deflate sigue siendo el predeterminado para que cualquier unzip lo abra.'],
      ['Arranque', 'Arranca en menos de un milisegundo', 'Compilado con LTO y una sola unidad de generación de código, sin runtime que arrancar.'],
      ['Formatos', 'Habla los formatos que usas', 'ZIP con Zip64, 7z con LZMA2 y nombres ocultos opcionales, TAR, gzip y XZ. Lee 7z sólidos; crea bloques independientes. Editar archivos existentes sigue siendo exclusivo de ZIP.'],
      ['Escritorio', 'Una ventana que respeta el teclado', 'Navega por los archivos como si fueran carpetas, marca con Space, filtra con Ctrl+F y vuelve atrás con Alt+←. Los lectores de pantalla leen cada fila.'],
      ['Portable', 'Una compilación en Rust puro', 'Quédate con libzstd nativo para la máxima velocidad, o cambia una opción para compilar sin ninguna dependencia de C.'],
    ],
  },
};


type IconType = ComponentType<SVGProps<SVGSVGElement>>;

function CardHeader({
  icon: Icon,
  eyebrow,
  title,
  children,
}: {
  icon: IconType;
  eyebrow?: string;
  title: ReactNode;
  children: ReactNode;
}) {
  return (
    <div>
      <div className="flex items-center gap-3">
        <span className="flex size-10 items-center justify-center rounded-xl border border-white/10 bg-white/[0.04] text-brand-300 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
          <Icon className="size-5" />
        </span>
        {eyebrow && (
          <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-slate-500">{eyebrow}</span>
        )}
      </div>
      <h3 className="mt-5 text-xl font-semibold tracking-[-0.02em] text-white sm:text-[22px]">{title}</h3>
      <p className="mt-2 text-[15px] leading-relaxed text-slate-400">{children}</p>
    </div>
  );
}

function MiniStat({ value, label }: { value: string; label: string }) {
  return (
    <div>
      <div className="text-lg font-semibold tracking-tight text-white sm:text-xl">{value}</div>
      <div className="mt-0.5 text-xs text-slate-500">{label}</div>
    </div>
  );
}

/* ---------------- Threads -------------------------------------------- */
const THREAD_OPTIONS = [1, 2, 4, 8, 16];

function ThreadsVisual() {
  const [threads, setThreads] = useState(8);
  const t = useCopy(COPY);
  const [ref, inView] = useInView<HTMLDivElement>();
  return (
    <div ref={ref} className="mt-8">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div role="group" aria-label={t.threadsGroup} className="inline-flex rounded-full border border-white/10 bg-black/30 p-1">
          {THREAD_OPTIONS.map((n) => (
            <button
              key={n}
              type="button"
              aria-pressed={threads === n}
              aria-label={t.threadLabel(n)}
              onClick={() => setThreads(n)}
              className={cn(
                'h-8 min-w-10 rounded-full px-3 font-mono text-xs transition-all duration-300',
                threads === n ? 'bg-white text-ink-950 shadow-[0_4px_14px_-4px_rgba(255,255,255,0.5)]' : 'text-slate-400 hover:text-white',
              )}
            >
              {n === 16 ? t.all : n}
            </button>
          ))}
        </div>
        <code className="rounded-lg border border-white/[0.06] bg-black/30 px-3 py-1.5 font-mono text-xs text-slate-300">
          <span className="text-brand-300">arca</span> <span className="text-sky-300">create</span> out.zip src/{' '}
          <span className="text-violet-300">-j</span> {threads === 16 ? '0' : threads}
        </code>
      </div>

      <div className="mt-6 grid grid-cols-8 gap-1.5 sm:gap-2" aria-hidden="true">
        {Array.from({ length: 16 }, (_, i) => {
          const on = inView && i < threads;
          return (
            <div
              key={i}
              className={cn(
                'relative h-16 overflow-hidden rounded-lg border transition-colors duration-500 sm:h-20',
                on ? 'border-brand-400/30 bg-brand-500/[0.07]' : 'border-white/[0.05] bg-white/[0.015]',
              )}
              style={{ transitionDelay: `${i * 30}ms` }}
            >
              <div
                className={cn(
                  'absolute inset-x-1 bottom-1 top-1 origin-bottom rounded-md bg-linear-to-t from-brand-600 via-brand-400 to-brand-200 transition-opacity duration-500',
                  on ? 'animate-core opacity-90' : 'opacity-0',
                )}
                style={{
                  animationDelay: `${-((i * 211) % 1400)}ms`,
                  animationDuration: `${1100 + (i % 5) * 160}ms`,
                  transitionDelay: `${i * 30}ms`,
                }}
              />
              <span className="absolute left-1.5 top-1 z-10 font-mono text-[9px] text-slate-500/80">{i}</span>
            </div>
          );
        })}
      </div>

      <div className="mt-6 grid grid-cols-3 gap-3 border-t border-white/[0.06] pt-5">
        <MiniStat value="1.79×" label={t.onTwo} />
        <MiniStat value="90%" label={t.efficiency} />
        <MiniStat value="0.207 s" label={t.extract} />
      </div>
      <MethodLink section="requirements" />
    </div>
  );
}

/* ---------------- Safety --------------------------------------------- */
const ENTRIES = [
  { name: 'assets/logo.svg', ok: true },
  { name: 'src/main.rs', ok: true },
  { name: '../../.ssh/authorized_keys', ok: false },
  { name: 'docs/guide.md', ok: true },
];

function SafeVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const t = useCopy(COPY);
  return (
    <div ref={ref} className="relative mt-6 overflow-hidden rounded-2xl border border-white/[0.06] bg-black/30 p-4 font-mono text-[12px]">
      <div className="mb-3 text-violet-300">#![forbid(unsafe_code)]</div>
      <ul className="space-y-1.5">
        {ENTRIES.map((e, i) => (
          <li
            key={e.name}
            className={cn(
              'flex items-center justify-between gap-3 rounded-md px-2 py-1 transition-all duration-500',
              inView ? 'translate-x-0 opacity-100' : '-translate-x-2 opacity-0',
              !e.ok && 'bg-rose-500/[0.08] ring-1 ring-inset ring-rose-400/25',
            )}
            style={{ transitionDelay: `${300 + i * 180}ms` }}
          >
            <span className={cn('truncate', e.ok ? 'text-slate-400' : 'text-rose-300')}>{e.name}</span>
            <span className={cn('shrink-0', e.ok ? 'text-emerald-400' : 'text-rose-300')}>{e.ok ? 'ok' : t.rejected}</span>
          </li>
        ))}
      </ul>
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 h-10 animate-scan bg-linear-to-b from-transparent via-brand-400/10 to-transparent"
      />
    </div>
  );
}

/* ---------------- Encryption ----------------------------------------- */
function CipherVisual() {
  const [on, setOn] = useState(false);
  const t = useCopy(COPY);
  const plain = t.plain;
  const text = useScramble(plain, on);
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={() => setOn((v) => !v)}
      onPointerEnter={(e) => e.pointerType === 'mouse' && setOn(true)}
      onPointerLeave={(e) => e.pointerType === 'mouse' && setOn(false)}
      className="mt-6 block w-full rounded-2xl border border-white/[0.06] bg-black/30 p-4 text-left transition duration-300 hover:border-brand-400/25"
    >
      <span className="flex items-center justify-between font-mono text-[11px] text-slate-500">
        <span>report.txt</span>
        <span className={cn('inline-flex items-center gap-1.5 transition-colors duration-300', on ? 'text-emerald-400' : 'text-slate-500')}>
          {on ? <Check className="size-3.5" /> : <Lock className="size-3.5" />}
          {on ? t.verified : 'AES-256-CTR'}
        </span>
      </span>
      <span
        aria-hidden="true"
        className={cn('mt-3 block truncate font-mono text-[15px] tracking-tight transition-colors duration-300', on ? 'text-white' : 'text-brand-300/80')}
      >
        {text}
      </span>
      <span className="sr-only">{on ? t.decrypted(plain) : t.encrypted}</span>
      <span className="mt-3 block text-[11px] text-slate-500">{on ? t.tapLock : t.tapOpen}</span>
    </button>
  );
}

/* ---------------- Zstandard ------------------------------------------ */
const ZSTD_ROWS = [
  { label: 'Arca · zstd', ms: 436, arca: true },
  { label: 'Arca · deflate', ms: 1012, arca: true },
  { label: 'zip -6', ms: 5746, arca: false },
  { label: '7-Zip -mx5', ms: 7877, arca: false },
];

function ZstdVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const t = useCopy(COPY);
  return (
    <div ref={ref} className="mt-6 space-y-3">
      {ZSTD_ROWS.map((r, i) => (
        <div key={r.label} className="text-xs">
          <div className="flex justify-between text-slate-500">
            <span className={r.arca ? 'text-slate-200' : ''}>{r.label}</span>
            <span className="font-mono tabular-nums">{r.ms.toLocaleString('en-US')} ms</span>
          </div>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-white/[0.05]">
            <div
              className={cn(
                'h-full rounded-full transition-[width] duration-[1400ms] ease-out',
                r.arca ? 'bg-linear-to-r from-brand-300 to-brand-500' : 'bg-slate-600',
              )}
              style={{ width: inView ? `${Math.max(3, (r.ms / 7877) * 100)}%` : '0%', transitionDelay: `${i * 120}ms` }}
            />
          </div>
        </div>
      ))}
      <p className="pt-1 text-[11px] text-slate-600">{t.zstdCaption}</p>
      <MethodLink section="compression" />
    </div>
  );
}

/* ---------------- Cold start ----------------------------------------- */
function ColdStartVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const v = useCountUp(0.8, inView, { duration: 1300, decimals: 1 });
  const t = useCopy(COPY);
  return (
    <div ref={ref} className="mt-6">
      <div className="flex items-baseline gap-2">
        <span className="brand-text text-6xl font-semibold tracking-[-0.05em] tabular-nums">{v}</span>
        <span className="text-xl text-slate-400">ms</span>
      </div>
      <div className="mt-5 space-y-2 font-mono text-xs text-slate-500">
        <div className="flex justify-between border-b border-white/[0.05] pb-2">
          <span>unzip</span>
          <span>2.0 ms</span>
        </div>
        <div className="flex justify-between border-b border-white/[0.05] pb-2">
          <span>7z</span>
          <span>2.5 ms</span>
        </div>
        <div className="flex justify-between">
          <span>{t.target}</span>
          <span>&lt; 15 ms</span>
        </div>
      </div>
      <MethodLink section="requirements" />
    </div>
  );
}

/* ---------------- Formats -------------------------------------------- */
const FORMATS = ['.zip', 'Zip64', '.tar (ustar)', '.tar.gz', '.tgz', '.tar.xz', '.xz', 'Deflate', 'Zstandard', 'Store', 'AES-256'];

function FormatsVisual() {
  const t = useCopy(COPY);
  return (
    <ul className="mt-6 flex flex-wrap gap-2">
      {[...FORMATS, t.readOnly].map((f, i) => (
        <li
          key={f}
          className={cn(
            'cursor-default rounded-full border px-3 py-1.5 font-mono text-xs transition-all duration-300 hover:-translate-y-0.5 hover:border-brand-400/40 hover:text-white',
            i === 0 ? 'border-brand-400/30 bg-brand-500/10 text-brand-200' : 'border-white/10 bg-white/[0.03] text-slate-400',
          )}
        >
          {f}
        </li>
      ))}
    </ul>
  );
}

/* ---------------- Keyboard ------------------------------------------- */
const SHORTCUTS = [['↑', '↓'], ['Enter'], ['Space'], ['Ctrl', 'A'], ['Alt', '←'], ['Ctrl', 'F']];

function KeyboardVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const reduced = usePrefersReducedMotion();
  const [active, setActive] = useState(0);
  const t = useCopy(COPY);
  useEffect(() => {
    if (!inView || reduced) return;
    const id = window.setInterval(() => setActive((a) => (a + 1) % SHORTCUTS.length), 1100);
    return () => window.clearInterval(id);
  }, [inView, reduced]);

  return (
    <div ref={ref} className="mt-6">
      <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3">
        {SHORTCUTS.map((keys, i) => (
          <li
            key={i}
            className={cn(
              'flex items-center justify-between gap-2 rounded-xl border px-3 py-2.5 transition-all duration-500',
              active === i ? 'border-brand-400/35 bg-brand-500/[0.08]' : 'border-white/[0.06] bg-black/20',
            )}
          >
            <span className="flex gap-1">
              {keys.map((k) => (
                <Kbd key={k} className={cn('transition-transform duration-300', active === i && 'translate-y-px border-brand-400/40 text-brand-100')}>
                  {k}
                </Kbd>
              ))}
            </span>
            <span className="text-xs text-slate-500">{t.shortcuts[i]}</span>
          </li>
        ))}
      </ul>
      <div className="mt-4 flex flex-wrap gap-2 text-[11px] text-slate-500">
        {[t.gpu, 'AccessKit', 'Narrator', 'NVDA'].map((tag) => (
          <span key={tag} className="rounded-full border border-white/[0.06] px-2.5 py-1">
            {tag}
          </span>
        ))}
      </div>
    </div>
  );
}

/* ---------------- Build profiles ------------------------------------- */
const PROFILES = [
  { id: 'native', cmd: 'cargo build --release' },
  { id: 'pure', cmd: 'cargo build --release --no-default-features' },
];

function BuildVisual() {
  const [sel, setSel] = useState(0);
  const t = useCopy(COPY);
  const p = PROFILES[sel];
  const labels = ['codecs-native', t.pure];
  return (
    <div className="mt-6">
      <div role="group" aria-label={t.profileGroup} className="inline-flex rounded-full border border-white/10 bg-black/30 p-1">
        {PROFILES.map((pr, i) => (
          <button
            key={pr.id}
            type="button"
            aria-pressed={sel === i}
            onClick={() => setSel(i)}
            className={cn(
              'h-8 rounded-full px-3.5 font-mono text-xs transition-all duration-300',
              sel === i ? 'bg-white text-ink-950' : 'text-slate-400 hover:text-white',
            )}
          >
            {labels[i]}
          </button>
        ))}
      </div>
      <div className="mt-4 flex items-center gap-2 rounded-xl border border-white/[0.06] bg-black/30 py-1.5 pl-4 pr-1.5">
        <code key={p.id} className="min-w-0 flex-1 animate-fade-in truncate font-mono text-[12.5px] text-slate-200">
          <span className="select-none text-slate-600">$ </span>
          <span className="text-sky-300">cargo</span> build <span className="text-violet-300">--release</span>
          {p.id === 'pure' && <span className="text-violet-300"> --no-default-features</span>}
        </code>
        <CopyButton text={p.cmd} label={t.copyBuild(labels[sel])} />
      </div>
      <p key={`n-${p.id}`} className="mt-3 animate-fade-in text-sm text-slate-500">
        {t.notes[sel]}
      </p>
    </div>
  );
}

/* ---------------- Section -------------------------------------------- */
export function Features() {
  const t = useCopy(COPY);
  const card = (i: number) => ({ eyebrow: t.cards[i][0], title: t.cards[i][1] });
  return (
    <section id="features" aria-labelledby="features-title" className="relative py-24 sm:py-32">
      <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 top-40 -z-10 h-[500px] bg-[radial-gradient(ellipse_at_center,rgba(47,107,255,0.07),transparent_60%)]" />
      <Container>
        <SectionHeading
          eyebrow={t.eyebrow}
          title={
            <span id="features-title">
              {t.title[0]}<Accent>{t.title[1]}</Accent>{t.title[2]}
            </span>
          }
          description={t.description}
        />

        <div className="mt-16 grid grid-cols-1 gap-4 md:grid-cols-6 lg:grid-cols-12">
          <Reveal className="md:col-span-6 lg:col-span-7 lg:row-span-2">
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Cpu} {...card(0)}>
                {t.cards[0][2]}
              </CardHeader>
              <ThreadsVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-5" delay={80}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={ShieldCheck} {...card(1)}>
                {t.cards[1][2]}
              </CardHeader>
              <SafeVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-5" delay={160}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Lock} {...card(2)}>
                {t.cards[2][2]}
              </CardHeader>
              <CipherVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-4">
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Zap} {...card(3)}>
                {t.cards[3][2]}
              </CardHeader>
              <ZstdVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-4" delay={80}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Timer} {...card(4)}>
                {t.cards[4][2]}
              </CardHeader>
              <ColdStartVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-6 lg:col-span-4" delay={160}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={FileArchive} {...card(5)}>
                {t.cards[5][2].split('arca password')[0]}<code>arca password</code>{t.cards[5][2].split('arca password')[1]}
              </CardHeader>
              <FormatsVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-6">
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Keyboard} {...card(6)}>
                {t.cards[6][2]}
              </CardHeader>
              <KeyboardVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-6" delay={80}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Wrench} {...card(7)}>
                {t.cards[7][2]}
              </CardHeader>
              <BuildVisual />
            </SpotlightCard>
          </Reveal>
        </div>
      </Container>
    </section>
  );
}
