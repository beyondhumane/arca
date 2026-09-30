import { useEffect, useRef, useState } from 'react';
import type { ComponentType, ReactNode, SVGProps } from 'react';
import { ArrowRight, Check, Lock, ShieldCheck, Timer } from 'lucide-react';
import { cn } from '@/utils/cn';
import { formatCount, useGitHubStars, useInView, usePrefersReducedMotion } from '@/lib/hooks';
import { RELEASE_URL, REPO_URL, VERSION } from '@/lib/site';
import { Accent, ButtonLink, Container } from '../ui';
import { DownloadButton } from '../download';
import { GitHubIcon } from '../icons';
import { Cursor, Prompt, TerminalWindow, colorizeCommand } from '../terminal';

/* ------------------------------------------------------------------ */
/*  Terminal script — real arca 0.7.2 output on the Silesia corpus,    */
/*  Ryzen 9 5900X, Linux. Commands: docs/guide/benchmarks.md.          */
/* ------------------------------------------------------------------ */
type Scene = { cmd: string; out: ReactNode[]; hold?: number };

const SCENES: Scene[] = [
  {
    cmd: 'arca create silesia.zip silesia -c zstd',
    out: [
      <>
        <span className="text-white">silesia.zip</span>: 12 files, 202.1 MB <span className="text-slate-500">-&gt;</span>{' '}
        <span className="text-brand-300">63.1 MB</span> <span className="text-emerald-400">(68.8% smaller)</span> in{' '}
        <span className="text-brand-300">0.263 s</span> <span className="text-slate-600">·</span> 770 MB/s{' '}
        <span className="text-slate-600">·</span> 24 threads
      </>,
    ],
    hold: 2200,
  },
  {
    cmd: 'arca list silesia.zip --time',
    out: [
      <span className="whitespace-pre text-slate-400">{'    51220480     zstd   64.4%  silesia/mozilla'}</span>,
      <span className="whitespace-pre text-slate-400">{'    41458703     zstd   70.8%  silesia/webster'}</span>,
      <span className="whitespace-pre text-slate-400">{'    33553445     zstd   91.6%  silesia/nci'}</span>,
      <span className="text-slate-600">…</span>,
      <span className="text-slate-500">
        12 entries, 202.1 MB uncompressed, listed in <span className="text-brand-300">0.1 ms</span>
      </span>,
    ],
    hold: 1900,
  },
  {
    cmd: 'arca extract silesia.zip -o out',
    out: [
      <>
        12 files, <span className="text-brand-300">202.1 MB</span> written in <span className="text-brand-300">0.067 s</span>
      </>,
    ],
    hold: 2600,
  },
];

function SceneBlock({ scene, typed, shown, cursor }: { scene: Scene; typed: string; shown: number; cursor?: boolean }) {
  return (
    <div className="mb-3">
      <div className="break-all">
        <Prompt />
        {colorizeCommand(typed)}
        {cursor && <Cursor />}
      </div>
      {scene.out.slice(0, shown).map((line, i) => (
        <div key={i} className="animate-fade-in break-words text-slate-300">
          {line}
        </div>
      ))}
    </div>
  );
}

function HeroTerminal() {
  const reduced = usePrefersReducedMotion();
  const [viewRef, inView] = useInView<HTMLDivElement>();
  const bodyRef = useRef<HTMLDivElement>(null);
  const [done, setDone] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [typed, setTyped] = useState('');
  const [shown, setShown] = useState(0);

  useEffect(() => {
    if (reduced || !inView) return;
    let cancelled = false;
    const timers = new Set<number>();
    const wait = (ms: number) =>
      new Promise<void>((resolve) => {
        const t = window.setTimeout(() => {
          timers.delete(t);
          resolve();
        }, ms);
        timers.add(t);
      });

    const run = async () => {
      await wait(1100);
      while (!cancelled) {
        for (let s = 0; s < SCENES.length; s++) {
          if (cancelled) return;
          const scene = SCENES[s];
          setActive(s);
          setTyped('');
          setShown(0);
          for (let i = 1; i <= scene.cmd.length; i++) {
            if (cancelled) return;
            setTyped(scene.cmd.slice(0, i));
            await wait(26 + Math.random() * 48);
          }
          await wait(440);
          for (let j = 1; j <= scene.out.length; j++) {
            if (cancelled) return;
            setShown(j);
            await wait(j === 1 ? 240 : 80);
          }
          await wait(scene.hold ?? 1800);
          if (cancelled) return;
          setDone((d) => [...d, s]);
          setActive(null);
          await wait(420);
        }
        await wait(900);
        if (cancelled) return;
        setDone([]);
      }
    };
    run();
    return () => {
      cancelled = true;
      timers.forEach((t) => window.clearTimeout(t));
    };
  }, [reduced, inView]);

  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
  }, [done, typed, shown, active]);

  const staticMode = reduced;

  return (
    <div ref={viewRef}>
      <TerminalWindow
        bodyRef={bodyRef}
        label="Terminal showing Arca creating, listing and extracting archives"
        bodyClassName="h-[300px] overflow-hidden sm:h-[340px]"
      >
        {staticMode ? (
          <>
            {SCENES.map((s, i) => (
              <SceneBlock key={i} scene={s} typed={s.cmd} shown={s.out.length} />
            ))}
            <Prompt />
            <Cursor />
          </>
        ) : (
          <>
            {done.map((s) => (
              <SceneBlock key={`d${s}`} scene={SCENES[s]} typed={SCENES[s].cmd} shown={SCENES[s].out.length} />
            ))}
            {active !== null ? (
              <SceneBlock key={`a${active}`} scene={SCENES[active]} typed={typed} shown={shown} cursor={shown === 0} />
            ) : (
              <div>
                <Prompt />
                <Cursor />
              </div>
            )}
          </>
        )}
      </TerminalWindow>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Floating decorations                                               */
/* ------------------------------------------------------------------ */
type IconType = ComponentType<SVGProps<SVGSVGElement>>;

function FloatingChip({
  className,
  icon: Icon,
  label,
  value,
  delay,
  mono,
}: {
  className?: string;
  icon: IconType;
  label: string;
  value: string;
  delay: string;
  mono?: boolean;
}) {
  return (
    <div className={cn('absolute z-20 hidden lg:block', className)} aria-hidden="true">
      <div className="animate-float" style={{ animationDelay: delay }}>
        <div className="glass-dark flex items-center gap-3 rounded-2xl py-2.5 pl-2.5 pr-4 shadow-[0_24px_60px_-18px_rgba(0,0,0,0.9)]">
          <span className="flex size-9 items-center justify-center rounded-xl bg-brand-500/10 text-brand-300 ring-1 ring-inset ring-brand-400/25">
            <Icon className="size-[18px]" />
          </span>
          <span className="text-left">
            <span className="block text-[10.5px] font-medium uppercase tracking-[0.14em] text-slate-500">{label}</span>
            <span className={cn('block text-sm font-semibold text-white', mono && 'font-mono text-[12.5px]')}>{value}</span>
          </span>
        </div>
      </div>
    </div>
  );
}

function RatioCard({ className }: { className?: string }) {
  const [ref, inView] = useInView<HTMLDivElement>();
  const pct = 68.8;
  const r = 26;
  const c = 2 * Math.PI * r;
  return (
    <div ref={ref} className={cn('absolute z-20 hidden lg:block', className)} aria-hidden="true">
      <div className="animate-float-slow">
        <div className="glass-dark w-[252px] rounded-2xl p-4 shadow-[0_30px_70px_-20px_rgba(0,0,0,0.9)]">
          <div className="flex items-center justify-between">
            <span className="font-mono text-[11px] text-slate-400">silesia.zip</span>
            <span className="rounded-full bg-emerald-400/10 px-2 py-0.5 text-[10px] font-medium text-emerald-300 ring-1 ring-inset ring-emerald-400/20">
              zstd · normal
            </span>
          </div>
          <div className="mt-3 flex items-center gap-4">
            <svg viewBox="0 0 64 64" className="size-16 -rotate-90">
              <defs>
                <linearGradient id="hero-ratio-grad" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse">
                  <stop stopColor="#8CC4FF" />
                  <stop offset="0.6" stopColor="#3B9CFF" />
                  <stop offset="1" stopColor="#FF8A3D" />
                </linearGradient>
              </defs>
              <circle cx="32" cy="32" r={r} fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="6" />
              <circle
                cx="32"
                cy="32"
                r={r}
                fill="none"
                stroke="url(#hero-ratio-grad)"
                strokeWidth="6"
                strokeLinecap="round"
                strokeDasharray={c}
                strokeDashoffset={inView ? c * (1 - pct / 100) : c}
                style={{ transition: 'stroke-dashoffset 1.8s cubic-bezier(0.22,1,0.36,1) 1.2s' }}
              />
            </svg>
            <div>
              <div className="text-2xl font-semibold tracking-tight text-white">68.8%</div>
              <div className="text-xs text-slate-400">smaller · 202.1 → 63.1 MB</div>
            </div>
          </div>
          <div className="mt-3 h-px bg-white/[0.06]" />
          <div className="mt-3 flex justify-between font-mono text-[11px] text-slate-500">
            <span>0.263 s</span>
            <span>770 MB/s</span>
            <span>24 threads</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function HeroVisual() {
  const tiltRef = useRef<HTMLDivElement>(null);
  const reduced = usePrefersReducedMotion();

  useEffect(() => {
    const el = tiltRef.current;
    if (!el) return;
    if (reduced) {
      el.style.setProperty('--tilt', '0');
      return;
    }
    let raf = 0;
    const update = () => {
      raf = 0;
      const r = el.getBoundingClientRect();
      const vh = window.innerHeight || 1;
      const p = Math.min(1, Math.max(0, (vh - r.top) / (vh * 0.85)));
      el.style.setProperty('--tilt', (1 - p).toFixed(3));
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(update);
    };
    update();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    return () => {
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [reduced]);

  return (
    <div
      className="relative mx-auto mt-16 max-w-4xl animate-fade-up [perspective:2000px] sm:mt-20"
      style={{ animationDelay: '900ms' }}
    >
      <div
        aria-hidden="true"
        className="absolute -inset-x-16 top-8 -z-10 h-[75%] rounded-[50%] bg-[radial-gradient(closest-side,rgba(47,107,255,0.32),transparent)] blur-2xl"
      />
      <div
        ref={tiltRef}
        className="relative origin-top transition-transform duration-300 ease-out"
        style={{ transform: 'rotateX(calc(var(--tilt, 0) * 16deg)) scale(calc(1 - var(--tilt, 0) * 0.05))' }}
      >
        <HeroTerminal />
        <FloatingChip className="-left-10 top-16 xl:-left-24" icon={Timer} label="Cold start" value="0.8 ms" delay="0s" />
        <FloatingChip className="-right-8 top-10 xl:-right-20" icon={Lock} label="Encryption" value="AES-256 · AE-2" delay="-2.4s" />
        <FloatingChip
          className="-left-12 bottom-6 xl:-left-28"
          icon={ShieldCheck}
          label="Every parser"
          value="#![forbid(unsafe_code)]"
          delay="-4.2s"
          mono
        />
        <RatioCard className="-bottom-12 -right-6 xl:-right-24" />
      </div>
    </div>
  );
}

function HeroBackground() {
  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 overflow-hidden">
      <div className="pattern-grid fade-radial absolute inset-0 opacity-70" />
      <div className="absolute left-1/2 top-[-20%] h-[700px] w-[1100px] -translate-x-1/2 animate-aurora rounded-full bg-[radial-gradient(closest-side,rgba(47,107,255,0.26),transparent_70%)]" />
      <div className="absolute right-[-14%] top-[6%] h-[560px] w-[680px] animate-aurora-slow rounded-full bg-[radial-gradient(closest-side,rgba(255,138,61,0.17),transparent_70%)]" />
      <div
        className="absolute left-[-16%] top-[24%] h-[560px] w-[700px] animate-aurora-slow rounded-full bg-[radial-gradient(closest-side,rgba(59,156,255,0.14),transparent_70%)]"
        style={{ animationDelay: '-22s' }}
      />
      <div className="absolute left-1/2 top-0 h-[620px] w-[1500px] -translate-x-1/2 bg-[conic-gradient(from_180deg_at_50%_0%,transparent_43%,rgba(140,196,255,0.085)_50%,transparent_57%)]" />
      <div className="absolute inset-x-0 top-0 h-px bg-linear-to-r from-transparent via-white/20 to-transparent" />
      <div className="absolute inset-x-0 bottom-0 h-56 bg-linear-to-b from-transparent to-ink-950" />
    </div>
  );
}

function Word({ children, delay }: { children: ReactNode; delay: number }) {
  return (
    <span className="inline-block animate-fade-up" style={{ animationDelay: `${delay}ms` }}>
      {children}
    </span>
  );
}

/* ------------------------------------------------------------------ */
/*  Hero                                                               */
/* ------------------------------------------------------------------ */
export function Hero() {
  const stars = useGitHubStars();
  const showStars = stars !== null && stars >= 10;

  return (
    <section id="top" aria-labelledby="hero-title" className="relative isolate overflow-hidden pb-24 pt-32 sm:pb-32 sm:pt-40">
      <HeroBackground />
      <Container>
        <div className="mx-auto max-w-4xl text-center">
          <a
            href={RELEASE_URL}
            target="_blank"
            rel="noreferrer noopener"
            className="group inline-flex max-w-full animate-fade-up items-center gap-2 rounded-full border border-white/10 bg-white/[0.035] py-1 pl-1 pr-3 text-[12.5px] text-slate-300 backdrop-blur transition hover:border-brand-400/40 hover:bg-white/[0.06] sm:text-[13px]"
          >
            <span className="shrink-0 rounded-full bg-linear-to-r from-brand-300 to-brand-500 px-2 py-0.5 text-[11px] font-semibold text-ink-950">
              v{VERSION}
            </span>
            <span className="truncate">Release notes and checksums</span>
            <ArrowRight className="size-3.5 shrink-0 text-slate-500 transition group-hover:translate-x-0.5 group-hover:text-brand-300" />
          </a>

          <h1
            id="hero-title"
            className="mt-8 text-[48px] font-semibold leading-[0.98] tracking-[-0.05em] text-white sm:text-7xl lg:text-[92px]"
          >
            <span className="block">
              <Word delay={120}>Pack</Word> <Word delay={200}>faster.</Word>
            </span>
            <span className="block">
              <Word delay={320}>Unpack</Word>{' '}
              <Word delay={430}>
                <Accent className="tracking-[-0.02em]">safer.</Accent>
              </Word>
            </span>
          </h1>

          <p
            className="mx-auto mt-7 max-w-2xl animate-fade-up text-pretty text-base leading-relaxed text-slate-400 sm:text-lg"
            style={{ animationDelay: '560ms' }}
          >
            Arca is an open-source archiver written in safe Rust. It compresses with Zstandard on every core,
            encrypts with AES-256, and rejects a malformed archive with an error instead of corrupting memory.
          </p>

          <div
            className="mt-10 flex animate-fade-up flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center"
            style={{ animationDelay: '680ms' }}
          >
            <DownloadButton />
            <ButtonLink href={REPO_URL} external variant="glass" size="lg">
              <GitHubIcon className="size-[18px]" />
              Star on GitHub
              {showStars && stars !== null && (
                <span className="rounded-full bg-white/10 px-2 py-0.5 font-mono text-xs text-slate-200">
                  {formatCount(stars)}
                </span>
              )}
            </ButtonLink>
          </div>

          <div
            className="mt-6 flex animate-fade-up flex-wrap items-center justify-center gap-x-5 gap-y-2 text-[13px] text-slate-500"
            style={{ animationDelay: '800ms' }}
          >
            <span className="inline-flex items-center gap-1.5">
              <Check className="size-3.5 text-emerald-400" /> Windows, macOS &amp; Linux
            </span>
            <span className="inline-flex items-center gap-1.5">
              <Check className="size-3.5 text-emerald-400" /> Free &amp; Apache-2.0
            </span>
            <a
              href="#/docs/installation"
              className="inline-flex items-center gap-1 text-slate-400 underline decoration-white/20 underline-offset-4 transition hover:text-white hover:decoration-brand-400"
            >
              Install guide <ArrowRight className="size-3.5" />
            </a>
            <a
              href="#/docs/benchmarks"
              className="inline-flex items-center gap-1 text-slate-400 underline decoration-white/20 underline-offset-4 transition hover:text-white hover:decoration-brand-400"
            >
              How these numbers were measured <ArrowRight className="size-3.5" />
            </a>
          </div>
        </div>

        <HeroVisual />
      </Container>
    </section>
  );
}
