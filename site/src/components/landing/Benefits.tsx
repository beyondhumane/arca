import type { ComponentType, SVGProps } from 'react';
import { ArrowRight, MousePointerClick, ShieldCheck, Terminal, Workflow } from 'lucide-react';
import { Accent, ButtonLink, Container, Eyebrow, Reveal, SpotlightCard } from '../ui';
import { MethodLink } from './MethodLink';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

const BENEFITS: {
  persona: string;
  icon: IconType;
  title: string;
  text: string;
  proof: string;
  proofLabel: string;
  method?: string;
}[] = [
  {
    persona: 'Developers',
    icon: Terminal,
    title: 'A CLI that stays out of your way',
    text: 'One-letter aliases, predictable flags, a single summary line, and a non-zero exit code the moment anything fails.',
    proof: '0.8 ms',
    proofLabel: 'cold start',
    method: 'requirements',
  },
  {
    persona: 'CI & release engineering',
    icon: Workflow,
    title: 'Give your pipeline its minutes back',
    text: 'Zstandard packs 212 MB in 436 ms on two cores, and arca test verifies an artifact end to end without writing a byte to disk.',
    proof: '13.2×',
    proofLabel: 'faster than zip -6',
    method: 'compression',
  },
  {
    persona: 'Security teams',
    icon: ShieldCheck,
    title: 'Hostile archives are just errors',
    text: 'Parsers forbid unsafe code and bound every header read. Zip Slip paths are rejected, encrypted data is authenticated, and ZipCrypto, which is broken, is read but never written.',
    proof: '0',
    proofLabel: 'lines of unsafe in the parsers',
  },
  {
    persona: 'Everyone else',
    icon: MousePointerClick,
    title: 'A window the whole team can use',
    text: 'Double-click to open, drag a rectangle to select, right-click in Explorer to extract. Fully keyboard-driven, and readable by Narrator and NVDA.',
    proof: '3',
    proofLabel: 'desktop platforms',
  },
];

export function Benefits() {
  return (
    <section id="benefits" aria-labelledby="benefits-title" className="relative py-24 sm:py-32">
      <Container>
        <div className="grid gap-12 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] lg:gap-16">
          <div className="lg:sticky lg:top-28 lg:self-start">
            <Reveal>
              <Eyebrow>Why Arca</Eyebrow>
            </Reveal>
            <Reveal delay={90}>
              <h2
                id="benefits-title"
                className="mt-5 text-balance text-[34px] font-semibold leading-[1.06] tracking-[-0.035em] text-white sm:text-5xl lg:text-[56px]"
              >
                Built for people who move <Accent>a lot</Accent> of bytes.
              </h2>
            </Reveal>
            <Reveal delay={180}>
              <p className="mt-5 max-w-md text-pretty text-base leading-relaxed text-slate-400 sm:text-lg">
                When packing and unpacking are fast, you archive more often, verify every artifact and open the file
                someone just sent you without worrying about what’s inside.
              </p>
            </Reveal>
            <Reveal delay={260}>
              <div className="mt-8 flex flex-wrap items-center gap-3">
                <ButtonLink href="#/docs/quick-start" variant="primary">
                  Quick start <ArrowRight className="size-4 transition-transform group-hover/btn:translate-x-0.5" />
                </ButtonLink>
                <ButtonLink href="#performance" variant="ghost">
                  See the numbers
                </ButtonLink>
              </div>
            </Reveal>
          </div>

          <ul className="space-y-4">
            {BENEFITS.map((b, i) => (
              <li key={b.title}>
                <Reveal variant="right" delay={i * 90}>
                  <SpotlightCard className="group p-6 sm:p-8">
                    <div className="flex flex-col gap-6 sm:flex-row sm:items-start">
                      <div className="flex-1">
                        <div className="flex items-center gap-3">
                          <span className="flex size-10 items-center justify-center rounded-xl border border-white/10 bg-white/[0.04] text-brand-300 transition-transform duration-500 group-hover:-rotate-6 group-hover:scale-110">
                            <b.icon className="size-5" />
                          </span>
                          <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-slate-500">{b.persona}</span>
                        </div>
                        <h3 className="mt-5 text-xl font-semibold tracking-[-0.02em] text-white sm:text-[22px]">{b.title}</h3>
                        <p className="mt-2 text-[15px] leading-relaxed text-slate-400">{b.text}</p>
                      </div>
                      <div className="shrink-0 border-t border-white/[0.06] pt-5 sm:w-36 sm:border-l sm:border-t-0 sm:pl-6 sm:pt-0 sm:text-right">
                        <div className="brand-text text-4xl font-semibold tracking-[-0.04em]">{b.proof}</div>
                        <div className="mt-1 text-xs leading-snug text-slate-500">{b.proofLabel}</div>
                        {b.method && <MethodLink section={b.method} className="sm:justify-end" />}
                      </div>
                    </div>
                  </SpotlightCard>
                </Reveal>
              </li>
            ))}
          </ul>
        </div>
      </Container>
    </section>
  );
}
