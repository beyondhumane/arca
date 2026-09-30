import { useEffect, useState } from 'react';
import type { ComponentType, ReactNode, SVGProps } from 'react';
import { Check, Cpu, FileArchive, Keyboard, Lock, ShieldCheck, Timer, Wrench, Zap } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useCountUp, useInView, usePrefersReducedMotion, useScramble } from '@/lib/hooks';
import { Accent, Container, CopyButton, Kbd, Reveal, SectionHeading, SpotlightCard } from '../ui';

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
  const [ref, inView] = useInView<HTMLDivElement>();
  return (
    <div ref={ref} className="mt-8">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div role="group" aria-label="Number of threads" className="inline-flex rounded-full border border-white/10 bg-black/30 p-1">
          {THREAD_OPTIONS.map((n) => (
            <button
              key={n}
              type="button"
              aria-pressed={threads === n}
              aria-label={n === 16 ? 'Every core' : `${n} thread${n > 1 ? 's' : ''}`}
              onClick={() => setThreads(n)}
              className={cn(
                'h-8 min-w-10 rounded-full px-3 font-mono text-xs transition-all duration-300',
                threads === n ? 'bg-white text-ink-950 shadow-[0_4px_14px_-4px_rgba(255,255,255,0.5)]' : 'text-slate-400 hover:text-white',
              )}
            >
              {n === 16 ? 'all' : n}
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
        <MiniStat value="1.89×" label="on 2 threads" />
        <MiniStat value="94%" label="scaling efficiency" />
        <MiniStat value="0.207 s" label="to extract 287 MB" />
      </div>
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
            <span className={cn('shrink-0', e.ok ? 'text-emerald-400' : 'text-rose-300')}>{e.ok ? 'ok' : 'rejected'}</span>
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
  const plain = 'Q3 revenue up 18% — embargoed';
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
          {on ? 'HMAC verified' : 'AES-256-CTR'}
        </span>
      </span>
      <span
        aria-hidden="true"
        className={cn('mt-3 block truncate font-mono text-[15px] tracking-tight transition-colors duration-300', on ? 'text-white' : 'text-brand-300/80')}
      >
        {text}
      </span>
      <span className="sr-only">{on ? `Decrypted contents: ${plain}` : 'Encrypted contents. Activate to decrypt.'}</span>
      <span className="mt-3 block text-[11px] text-slate-500">{on ? 'Decrypted — tap again to lock' : 'Hover or tap to decrypt'}</span>
    </button>
  );
}

/* ---------------- Zstandard ------------------------------------------ */
const ZSTD_ROWS = [
  { label: 'Arca · zstd', ms: 330, arca: true },
  { label: 'Arca · deflate', ms: 941, arca: true },
  { label: 'zip -6', ms: 3934, arca: false },
  { label: '7-Zip, 2 threads', ms: 5358, arca: false },
];

function ZstdVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
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
              style={{ width: inView ? `${Math.max(3, (r.ms / 5358) * 100)}%` : '0%', transitionDelay: `${i * 120}ms` }}
            />
          </div>
        </div>
      ))}
      <p className="pt-1 text-[11px] text-slate-600">81 MB · 120 files · 2 threads · lower is better</p>
    </div>
  );
}

/* ---------------- Cold start ----------------------------------------- */
function ColdStartVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const v = useCountUp(1.6, inView, { duration: 1300, decimals: 1 });
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
          <span>3.7 ms</span>
        </div>
        <div className="flex justify-between">
          <span>design target (R1)</span>
          <span>&lt; 15 ms</span>
        </div>
      </div>
    </div>
  );
}

/* ---------------- Formats -------------------------------------------- */
const FORMATS = ['.zip', 'Zip64', '.tar (ustar)', '.tar.gz', '.tgz', 'Deflate', 'Zstandard', 'Store', 'AES-256', 'ZipCrypto · read-only'];

function FormatsVisual() {
  return (
    <ul className="mt-6 flex flex-wrap gap-2">
      {FORMATS.map((f, i) => (
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
const SHORTCUTS: { keys: string[]; label: string }[] = [
  { keys: ['↑', '↓'], label: 'move' },
  { keys: ['Enter'], label: 'open' },
  { keys: ['Space'], label: 'tick' },
  { keys: ['Ctrl', 'A'], label: 'tick all' },
  { keys: ['Alt', '←'], label: 'back' },
  { keys: ['Ctrl', 'F'], label: 'filter' },
];

function KeyboardVisual() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const reduced = usePrefersReducedMotion();
  const [active, setActive] = useState(0);
  useEffect(() => {
    if (!inView || reduced) return;
    const id = window.setInterval(() => setActive((a) => (a + 1) % SHORTCUTS.length), 1100);
    return () => window.clearInterval(id);
  }, [inView, reduced]);

  return (
    <div ref={ref} className="mt-6">
      <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3">
        {SHORTCUTS.map((s, i) => (
          <li
            key={s.label}
            className={cn(
              'flex items-center justify-between gap-2 rounded-xl border px-3 py-2.5 transition-all duration-500',
              active === i ? 'border-brand-400/35 bg-brand-500/[0.08]' : 'border-white/[0.06] bg-black/20',
            )}
          >
            <span className="flex gap-1">
              {s.keys.map((k) => (
                <Kbd key={k} className={cn('transition-transform duration-300', active === i && 'translate-y-px border-brand-400/40 text-brand-100')}>
                  {k}
                </Kbd>
              ))}
            </span>
            <span className="text-xs text-slate-500">{s.label}</span>
          </li>
        ))}
      </ul>
      <div className="mt-4 flex flex-wrap gap-2 text-[11px] text-slate-500">
        {['GPU-rendered with GPUI', 'AccessKit', 'Narrator', 'NVDA'].map((t) => (
          <span key={t} className="rounded-full border border-white/[0.06] px-2.5 py-1">
            {t}
          </span>
        ))}
      </div>
    </div>
  );
}

/* ---------------- Build profiles ------------------------------------- */
const PROFILES = [
  { id: 'native', label: 'codecs-native', cmd: 'cargo build --release', note: 'The default. Links libzstd (C) for the fastest Zstandard.' },
  { id: 'pure', label: 'pure Rust', cmd: 'cargo build --release --no-default-features', note: 'No C toolchain needed. Builds for any target Rust supports.' },
];

function BuildVisual() {
  const [sel, setSel] = useState(0);
  const p = PROFILES[sel];
  return (
    <div className="mt-6">
      <div role="group" aria-label="Build profile" className="inline-flex rounded-full border border-white/10 bg-black/30 p-1">
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
            {pr.label}
          </button>
        ))}
      </div>
      <div className="mt-4 flex items-center gap-2 rounded-xl border border-white/[0.06] bg-black/30 py-1.5 pl-4 pr-1.5">
        <code key={p.id} className="min-w-0 flex-1 animate-fade-in truncate font-mono text-[12.5px] text-slate-200">
          <span className="select-none text-slate-600">$ </span>
          <span className="text-sky-300">cargo</span> build <span className="text-violet-300">--release</span>
          {p.id === 'pure' && <span className="text-violet-300"> --no-default-features</span>}
        </code>
        <CopyButton text={p.cmd} label={`Copy ${p.label} build command`} />
      </div>
      <p key={`n-${p.id}`} className="mt-3 animate-fade-in text-sm text-slate-500">
        {p.note}
      </p>
    </div>
  );
}

/* ---------------- Section -------------------------------------------- */
export function Features() {
  return (
    <section id="features" aria-labelledby="features-title" className="relative py-24 sm:py-32">
      <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 top-40 -z-10 h-[500px] bg-[radial-gradient(ellipse_at_center,rgba(47,107,255,0.07),transparent_60%)]" />
      <Container>
        <SectionHeading
          eyebrow="Features"
          title={
            <span id="features-title">
              Everything an archiver should be. <Accent>Nothing</Accent> it shouldn’t.
            </span>
          }
          description="One small binary for the terminal, a native window for everyone else — and the same fast, careful core underneath both."
        />

        <div className="mt-16 grid grid-cols-1 gap-4 md:grid-cols-6 lg:grid-cols-12">
          <Reveal className="md:col-span-6 lg:col-span-7 lg:row-span-2">
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Cpu} eyebrow="Multi-threaded" title="Every core, on demand">
                Compression and extraction fan out across your CPU. A .zip is random access, so each thread opens its
                own entry — no locks, no shared stream. Pick a thread count and watch it scale.
              </CardHeader>
              <ThreadsVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-5" delay={80}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={ShieldCheck} eyebrow="Memory-safe" title="Safe by construction">
                Container parsers forbid unsafe code. Malformed input becomes an error, and Zip Slip paths are refused
                before a byte is written.
              </CardHeader>
              <SafeVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-5" delay={160}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Lock} eyebrow="Encryption" title="AES-256, authenticated">
                WinZip AE-2 with a fresh salt per entry and an HMAC over every byte. 7-Zip and WinRAR open it; tampering
                fails loudly.
              </CardHeader>
              <CipherVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-4">
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Zap} eyebrow="Codecs" title="Zstandard inside ZIP">
                Ask for zstd and get the smallest, fastest archive of the bunch. Deflate stays the default so any unzip
                can open it.
              </CardHeader>
              <ZstdVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-4" delay={80}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Timer} eyebrow="Startup" title="Instant, literally">
                LTO, one codegen unit and zero runtime baggage. Arca is done before your terminal finishes blinking.
              </CardHeader>
              <ColdStartVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-6 lg:col-span-4" delay={160}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={FileArchive} eyebrow="Formats" title="Speaks the formats you use">
                ZIP with Zip64 for giant archives, ustar TAR with checksum verification, gzip on top — and legacy
                ZipCrypto archives open, then upgrade.
              </CardHeader>
              <FormatsVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-6">
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Keyboard} eyebrow="Desktop" title="A window that respects the keyboard">
                Browse archives like folders, tick with Space, filter with Ctrl+F, and go back with Alt+←. Screen readers
                get every row.
              </CardHeader>
              <KeyboardVisual />
            </SpotlightCard>
          </Reveal>

          <Reveal className="md:col-span-3 lg:col-span-6" delay={80}>
            <SpotlightCard className="h-full p-6 sm:p-8">
              <CardHeader icon={Wrench} eyebrow="Portable" title="Pure Rust, any target">
                Keep native libzstd for peak speed, or flip one flag for a build with no C dependency at all.
              </CardHeader>
              <BuildVisual />
            </SpotlightCard>
          </Reveal>
        </div>
      </Container>
    </section>
  );
}
