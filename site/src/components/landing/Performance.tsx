import { useEffect, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';
import { ArrowRight, Scale } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useInView } from '@/lib/hooks';
import { Accent, Container, Reveal, SectionHeading } from '../ui';
import { MethodLink } from './MethodLink';

type Bar = { label: string; value: number; display: string; arca?: boolean; sub?: string };
type Group = { title?: string; bars: Bar[] };
type Dataset = {
  id: string;
  tab: string;
  title: string;
  context: string;
  method: string;
  groups: Group[];
  highlights: { value: string; label: string }[];
};

const DATASETS: Dataset[] = [
  {
    id: 'compress',
    tab: 'Compression',
    title: 'Compressing the Silesia corpus in 120 files',
    context: 'Ryzen 9 5900X · Linux · pinned to 2 cores · 212 MB',
    method: 'compression',
    groups: [
      {
        bars: [
          { label: 'Arca · zstd', value: 436, display: '436 ms', sub: '66.87 MB', arca: true },
          { label: 'Arca · deflate', value: 1012, display: '1,012 ms', sub: '67.62 MB', arca: true },
          { label: 'zip -6', value: 5746, display: '5,746 ms', sub: '68.34 MB' },
          { label: '7-Zip · zip -mx5', value: 7877, display: '7,877 ms', sub: '65.81 MB' },
        ],
      },
    ],
    highlights: [
      { value: '13.2×', label: 'faster than zip -6' },
      { value: '18.1×', label: 'faster than 7-Zip' },
      { value: '1.6%', label: 'larger than 7-Zip’s archive, the smallest' },
    ],
  },
  {
    id: 'extract',
    tab: 'Parallel extraction',
    title: 'Extracting 287 MB in 16 text files',
    context: 'Windows 11 · 16 threads · best of 3',
    method: 'parallel-extraction',
    groups: [
      {
        bars: [
          { label: 'Arca · 16 threads', value: 0.207, display: '0.207 s', arca: true },
          { label: 'Arca · -j 1', value: 0.686, display: '0.686 s', arca: true },
          { label: '7-Zip', value: 1.02, display: '1.020 s' },
        ],
      },
    ],
    highlights: [
      { value: '4.9×', label: 'faster than 7-Zip' },
      { value: '3.3×', label: 'gained from threads alone' },
      { value: '1 per core', label: 'entries decoded independently' },
    ],
  },
  {
    id: 'nanazip',
    tab: 'vs NanaZip · 3.13 GB',
    title: 'A 3.13 GB archive that unpacks to 6.28 GB',
    context: 'Windows 11 · 16 cores · NanaZip 7.0 · DRAM-less SATA SSD',
    method: 'against-nanazip-on-an-archive-big-enough-to-hurt',
    groups: [
      {
        title: 'Decode only: one core, nothing written',
        bars: [
          { label: 'Arca', value: 13.2, display: '13.2 s', arca: true },
          { label: 'NanaZip', value: 25.9, display: '25.9 s' },
        ],
      },
      {
        title: 'Full extraction, best run of each',
        bars: [
          { label: 'Arca', value: 21.9, display: '21.9 s', arca: true },
          { label: 'NanaZip', value: 49.0, display: '49.0 s' },
        ],
      },
    ],
    highlights: [
      { value: '2×', label: 'faster deflate decoder, same single core' },
      { value: '294 MB/s', label: 'against a 327 MB/s disk ceiling' },
      { value: '28.0 s', label: 'Arca’s worst run, faster than NanaZip’s best' },
    ],
  },
  {
    id: 'startup',
    tab: 'Startup & listing',
    title: 'Cold start and listing 6,000 entries',
    context: 'Ryzen 9 5900X · Linux · hyperfine mean',
    method: 'requirements',
    groups: [
      {
        title: 'R1 · cold start, list a one-entry zip · target < 15 ms',
        bars: [
          { label: 'Arca', value: 0.8, display: '0.8 ms', arca: true },
          { label: 'unzip', value: 2.0, display: '2.0 ms' },
          { label: '7z', value: 2.5, display: '2.5 ms' },
        ],
      },
      {
        title: 'R2 · list 6,000 entries · target < 200 ms',
        bars: [
          { label: 'Arca', value: 3.1, display: '3.1 ms', arca: true },
          { label: 'unzip', value: 18.6, display: '18.6 ms' },
          { label: '7z', value: 71.8, display: '71.8 ms' },
        ],
      },
    ],
    highlights: [
      { value: '23×', label: 'faster listing than 7z' },
      { value: '1.79×', label: 'R3 scaling on 2 threads' },
      { value: '90%', label: 'parallel efficiency' },
    ],
  },
];

function BarGroup({ group, active }: { group: Group; active: boolean }) {
  const [go, setGo] = useState(false);
  useEffect(() => {
    if (!active) return;
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => setGo(true));
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, [active]);

  const max = Math.max(...group.bars.map((b) => b.value));

  return (
    <div>
      {group.title && (
        <div className="mb-4 font-mono text-[11px] uppercase tracking-[0.14em] text-slate-500">{group.title}</div>
      )}
      <ul className="space-y-4 sm:space-y-3">
        {group.bars.map((b, i) => (
          <li key={b.label} className="sm:grid sm:grid-cols-[190px_minmax(0,1fr)_92px] sm:items-center sm:gap-4">
            <div className="flex items-baseline justify-between gap-3 sm:block">
              <span className={cn('text-sm', b.arca ? 'font-medium text-white' : 'text-slate-400')}>
                {b.label}
                {b.sub && <span className="ml-2 font-mono text-[11px] text-slate-600 sm:ml-0 sm:block">{b.sub}</span>}
              </span>
              <span className="font-mono text-sm tabular-nums text-slate-200 sm:hidden">{b.display}</span>
            </div>
            <div className="relative mt-2 h-2.5 overflow-hidden rounded-full bg-white/[0.04] sm:mt-0 sm:h-8 sm:rounded-lg">
              <div
                className={cn(
                  'absolute inset-y-0 left-0 overflow-hidden rounded-full transition-[width] duration-[1300ms] ease-[cubic-bezier(0.22,1,0.36,1)] sm:rounded-lg',
                  b.arca
                    ? 'bg-linear-to-r from-brand-400 via-brand-500 to-spark-500 shadow-[0_0_28px_-6px_rgba(47,107,255,0.7)]'
                    : 'bg-linear-to-r from-slate-700 to-slate-500',
                )}
                style={{ width: go ? `${Math.max(2.5, (b.value / max) * 100)}%` : '0%', transitionDelay: `${100 + i * 110}ms` }}
              >
                {b.arca && (
                  <span
                    aria-hidden="true"
                    className="absolute inset-0 animate-shimmer bg-[linear-gradient(110deg,transparent_30%,rgba(255,255,255,0.28)_50%,transparent_70%)] bg-[length:220%_100%]"
                  />
                )}
              </div>
            </div>
            <span className="hidden text-right font-mono text-sm tabular-nums text-slate-200 sm:block">{b.display}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function Performance() {
  const [sel, setSel] = useState(0);
  const [ref, inView] = useInView<HTMLDivElement>();
  const ds = DATASETS[sel];

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    let next = sel;
    if (e.key === 'ArrowRight') next = (sel + 1) % DATASETS.length;
    else if (e.key === 'ArrowLeft') next = (sel - 1 + DATASETS.length) % DATASETS.length;
    else return;
    e.preventDefault();
    setSel(next);
    document.getElementById(`bench-tab-${DATASETS[next].id}`)?.focus();
  };

  return (
    <section id="performance" aria-labelledby="performance-title" className="relative py-24 sm:py-32">
      <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 overflow-hidden">
        <div className="pattern-dots fade-radial absolute inset-0 opacity-50" />
      </div>
      <Container>
        <SectionHeading
          eyebrow="Performance"
          title={
            <span id="performance-title">
              Every number comes with its <Accent>command.</Accent>
            </span>
          }
          description="Each chart names the machine it ran on and links to the command behind it, including the runs Arca loses."
        />

        <Reveal delay={100}>
          <div
            role="tablist"
            aria-label="Benchmarks"
            onKeyDown={onKeyDown}
            className="no-scrollbar -mx-5 mt-12 flex gap-2 overflow-x-auto px-5 sm:mx-0 sm:flex-wrap sm:justify-center sm:px-0"
          >
            {DATASETS.map((d, i) => (
              <button
                key={d.id}
                id={`bench-tab-${d.id}`}
                type="button"
                role="tab"
                aria-selected={sel === i}
                aria-controls="bench-panel"
                tabIndex={sel === i ? 0 : -1}
                onClick={() => setSel(i)}
                className={cn(
                  'h-10 shrink-0 rounded-full border px-4 text-sm transition-all duration-300',
                  sel === i
                    ? 'border-brand-400/40 bg-brand-500/10 text-white shadow-[0_0_24px_-8px_rgba(47,107,255,0.6)]'
                    : 'border-white/10 bg-white/[0.02] text-slate-400 hover:border-white/20 hover:text-white',
                )}
              >
                {d.tab}
              </button>
            ))}
          </div>
        </Reveal>

        <Reveal delay={160}>
          <div ref={ref} className="mt-8 grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px]">
            <div
              id="bench-panel"
              role="tabpanel"
              aria-labelledby={`bench-tab-${ds.id}`}
              className="glass rounded-3xl p-6 shadow-[0_30px_80px_-40px_rgba(0,0,0,0.9)] sm:p-8"
            >
              <div key={ds.id} className="animate-fade-in">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="text-lg font-semibold tracking-[-0.02em] text-white sm:text-xl">{ds.title}</h3>
                    <p className="mt-1.5 font-mono text-xs text-slate-500">{ds.context}</p>
                    <MethodLink section={ds.method} className="mt-2" />
                  </div>
                  <span className="rounded-full border border-white/10 px-2.5 py-1 text-[11px] text-slate-400">Lower is better</span>
                </div>
                <div className="mt-8 space-y-9">
                  {ds.groups.map((g, i) => (
                    <BarGroup key={`${ds.id}-${i}`} group={g} active={inView} />
                  ))}
                </div>
              </div>
            </div>

            <div className="grid gap-4">
              <div className="rounded-3xl border border-white/[0.07] bg-linear-to-b from-brand-500/[0.08] to-transparent p-6">
                <ul key={ds.id} className="space-y-5">
                  {ds.highlights.map((h, i) => (
                    <li key={h.label} className="animate-fade-up" style={{ animationDelay: `${i * 90}ms` }}>
                      <div className="brand-text text-3xl font-semibold tracking-[-0.04em]">{h.value}</div>
                      <div className="mt-1 text-sm text-slate-400">{h.label}</div>
                    </li>
                  ))}
                </ul>
              </div>
              <div className="rounded-3xl border border-white/[0.07] bg-white/[0.02] p-6">
                <div className="flex items-center gap-2 text-sm font-medium text-white">
                  <Scale className="size-4 text-brand-300" /> Where it doesn’t win
                </div>
                <p className="mt-3 text-sm leading-relaxed text-slate-400">
                  5,358 small source files, deflate vs deflate on Windows 11: <span className="text-slate-200">7-Zip 0.627 s</span>,
                  Arca 0.710 s. With many tiny files, 97% of the time goes to creating files on NTFS, and every tool
                  pays that cost.
                </p>
                <a
                  href="#/docs/benchmarks/deflate-against-deflate-on-windows-11"
                  className="mt-4 inline-flex items-center gap-1 text-sm text-brand-300 transition hover:gap-2 hover:text-brand-200"
                >
                  Full methodology <ArrowRight className="size-3.5" />
                </a>
              </div>
            </div>
          </div>
        </Reveal>
      </Container>
    </section>
  );
}
