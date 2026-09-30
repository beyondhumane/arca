import type { ComponentType, SVGProps } from 'react';
import { Archive, Boxes, FileArchive, FolderOpen, Layers, Package, Zap } from 'lucide-react';
import { useCountUp, useInView } from '@/lib/hooks';
import { Container, Reveal } from '../ui';
import { RustIcon } from '../icons';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

const TOOLS: { name: string; icon: IconType }[] = [
  { name: '7-Zip', icon: Archive },
  { name: 'Info-ZIP unzip', icon: FileArchive },
  { name: 'GNU tar', icon: Package },
  { name: 'WinRAR', icon: Layers },
  { name: 'NanaZip', icon: Boxes },
  { name: 'Windows Explorer', icon: FolderOpen },
  { name: 'Zstandard', icon: Zap },
  { name: 'zlib-rs', icon: RustIcon },
];

const STATS = [
  { value: 164, label: 'tests across the workspace' },
  { value: 35, label: 'interop cases, verified by SHA-256' },
  { value: 0, label: 'lines of unsafe in the parsers' },
  { value: 3, label: 'platforms: Windows, macOS, Linux' },
];

function Stat({ value, label, index }: { value: number; label: string; index: number }) {
  const [ref, inView] = useInView<HTMLDivElement>();
  const display = useCountUp(value, inView, { duration: 1400 + index * 150 });
  return (
    <div ref={ref} className="group relative bg-ink-950 px-6 py-8 transition-colors duration-500 hover:bg-ink-900 sm:px-8 sm:py-10">
      <div
        aria-hidden="true"
        className="absolute inset-x-8 top-0 h-px bg-linear-to-r from-transparent via-brand-400/0 to-transparent transition-all duration-500 group-hover:via-brand-400/70"
      />
      <div className="silver-text text-4xl font-semibold tracking-[-0.04em] tabular-nums sm:text-5xl">{display}</div>
      <p className="mt-2 text-sm leading-snug text-slate-500">{label}</p>
    </div>
  );
}

export function SocialProof() {
  const loop = [...TOOLS, ...TOOLS];
  return (
    <section aria-labelledby="proof-title" className="relative py-16 sm:py-20">
      <Container>
        <Reveal>
          <p id="proof-title" className="text-center text-sm text-slate-500">
            Reads and writes the archives your team already trades — with the tools they already use
          </p>
        </Reveal>
      </Container>

      <Reveal delay={120}>
        <div className="fade-x group relative mt-8 overflow-hidden">
          <ul className="flex w-max animate-marquee items-center gap-12 pr-12 group-hover:[animation-play-state:paused] sm:gap-16 sm:pr-16">
            {loop.map((t, i) => (
              <li
                key={`${t.name}-${i}`}
                aria-hidden={i >= TOOLS.length ? true : undefined}
                className="flex shrink-0 items-center gap-2.5 text-slate-500 transition-colors duration-300 hover:text-slate-100"
              >
                <t.icon className="size-5" />
                <span className="text-lg font-semibold tracking-[-0.02em] sm:text-xl">{t.name}</span>
              </li>
            ))}
          </ul>
        </div>
      </Reveal>

      <Container>
        <Reveal delay={200}>
          <div className="mt-14 grid grid-cols-2 gap-px overflow-hidden rounded-3xl border border-white/[0.07] bg-white/[0.07] lg:grid-cols-4">
            {STATS.map((s, i) => (
              <Stat key={s.label} value={s.value} label={s.label} index={i} />
            ))}
          </div>
        </Reveal>
      </Container>
    </section>
  );
}
