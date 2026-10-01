import type { ComponentType, SVGProps } from 'react';
import { ArrowRight, MousePointerClick, ShieldCheck, Terminal, Workflow } from 'lucide-react';
import { Accent, ButtonLink, Container, Eyebrow, Reveal, SpotlightCard } from '../ui';
import { useCopy } from '@/lib/i18n';
import { MethodLink } from './MethodLink';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

const ICONS: IconType[] = [Terminal, Workflow, ShieldCheck, MousePointerClick];
const PROOF = [
  { proof: '0.8 ms', method: 'requirements' },
  { proof: '13.2×', method: 'compression' },
  { proof: '0' },
  { proof: '3' },
];

type Benefit = { persona: string; title: string; text: string; proofLabel: string };

const COPY: Record<'en' | 'es', { eyebrow: string; title: [string, string, string]; lead: string; quick: string; numbers: string; items: Benefit[] }> = {
  en: {
    eyebrow: 'Why Arca',
    title: ['Built for people who move ', 'a lot', ' of bytes.'],
    lead: 'When packing and unpacking are fast, you archive more often, verify every artifact and open the file someone just sent you without worrying about what’s inside.',
    quick: 'Quick start',
    numbers: 'See the numbers',
    items: [
      {
        persona: 'Developers',
        title: 'A CLI that stays out of your way',
        text: 'One-letter aliases, predictable flags, a single summary line, and a non-zero exit code the moment anything fails.',
        proofLabel: 'cold start',
      },
      {
        persona: 'CI & release engineering',
        title: 'Give your pipeline its minutes back',
        text: 'Zstandard packs 212 MB in 436 ms on two cores, and arca test verifies an artifact end to end without writing a byte to disk.',
        proofLabel: 'faster than zip -6',
      },
      {
        persona: 'Security teams',
        title: 'Hostile archives are just errors',
        text: 'Parsers forbid unsafe code and bound every header read. Zip Slip paths are rejected, encrypted data is authenticated, and ZipCrypto, which is broken, is read but never written.',
        proofLabel: 'lines of unsafe in the parsers',
      },
      {
        persona: 'Everyone else',
        title: 'A window the whole team can use',
        text: 'Double-click to open, drag a rectangle to select, right-click in Explorer to extract. Fully keyboard-driven, and readable by Narrator and NVDA.',
        proofLabel: 'desktop platforms',
      },
    ],
  },
  es: {
    eyebrow: 'Por qué Arca',
    title: ['Hecho para quien mueve ', 'muchos', ' bytes.'],
    lead: 'Cuando comprimir y extraer es rápido, archivas más a menudo, verificas cada artefacto y abres el archivo que te acaban de mandar sin preocuparte por lo que lleva dentro.',
    quick: 'Primeros pasos',
    numbers: 'Ver las cifras',
    items: [
      {
        persona: 'Desarrollo',
        title: 'Una CLI que no estorba',
        text: 'Alias de una letra, opciones predecibles, una sola línea de resumen y un código de salida distinto de cero en cuanto algo falla.',
        proofLabel: 'arranque en frío',
      },
      {
        persona: 'CI y publicación',
        title: 'Devuelve los minutos a tu pipeline',
        text: 'Zstandard comprime 212 MB en 436 ms con dos núcleos, y arca test verifica un artefacto entero sin escribir un byte en disco.',
        proofLabel: 'más rápido que zip -6',
      },
      {
        persona: 'Equipos de seguridad',
        title: 'Un archivo hostil es solo un error',
        text: 'Los analizadores prohíben el código unsafe y acotan cada cabecera que leen. Se rechazan las rutas Zip Slip, los datos cifrados se autentican, y ZipCrypto, que está roto, se lee pero nunca se escribe.',
        proofLabel: 'líneas unsafe en los analizadores',
      },
      {
        persona: 'Todos los demás',
        title: 'Una ventana para todo el equipo',
        text: 'Doble clic para abrir, arrastra un rectángulo para seleccionar, clic derecho en el Explorador para extraer. Todo funciona con el teclado y lo leen Narrador y NVDA.',
        proofLabel: 'plataformas de escritorio',
      },
    ],
  },
};

export function Benefits() {
  const t = useCopy(COPY);
  return (
    <section id="benefits" aria-labelledby="benefits-title" className="relative py-24 sm:py-32">
      <Container>
        <div className="grid gap-12 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] lg:gap-16">
          <div className="lg:sticky lg:top-28 lg:self-start">
            <Reveal>
              <Eyebrow>{t.eyebrow}</Eyebrow>
            </Reveal>
            <Reveal delay={90}>
              <h2
                id="benefits-title"
                className="mt-5 text-balance text-[34px] font-semibold leading-[1.06] tracking-[-0.035em] text-white sm:text-5xl lg:text-[56px]"
              >
                {t.title[0]}<Accent>{t.title[1]}</Accent>{t.title[2]}
              </h2>
            </Reveal>
            <Reveal delay={180}>
              <p className="mt-5 max-w-md text-pretty text-base leading-relaxed text-slate-400 sm:text-lg">
                {t.lead}
              </p>
            </Reveal>
            <Reveal delay={260}>
              <div className="mt-8 flex flex-wrap items-center gap-3">
                <ButtonLink href="#/docs/quick-start" variant="primary">
                  {t.quick} <ArrowRight className="size-4 transition-transform group-hover/btn:translate-x-0.5" />
                </ButtonLink>
                <ButtonLink href="#performance" variant="ghost">
                  {t.numbers}
                </ButtonLink>
              </div>
            </Reveal>
          </div>

          <ul className="space-y-4">
            {t.items.map((b, i) => {
              const Icon = ICONS[i];
              const { proof, method } = PROOF[i];
              return (
              <li key={i}>
                <Reveal variant="right" delay={i * 90}>
                  <SpotlightCard className="group p-6 sm:p-8">
                    <div className="flex flex-col gap-6 sm:flex-row sm:items-start">
                      <div className="flex-1">
                        <div className="flex items-center gap-3">
                          <span className="flex size-10 items-center justify-center rounded-xl border border-white/10 bg-white/[0.04] text-brand-300 transition-transform duration-500 group-hover:-rotate-6 group-hover:scale-110">
                            <Icon className="size-5" />
                          </span>
                          <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-slate-500">{b.persona}</span>
                        </div>
                        <h3 className="mt-5 text-xl font-semibold tracking-[-0.02em] text-white sm:text-[22px]">{b.title}</h3>
                        <p className="mt-2 text-[15px] leading-relaxed text-slate-400">{b.text}</p>
                      </div>
                      <div className="shrink-0 border-t border-white/[0.06] pt-5 sm:w-36 sm:border-l sm:border-t-0 sm:pl-6 sm:pt-0 sm:text-right">
                        <div className="brand-text text-4xl font-semibold tracking-[-0.04em]">{proof}</div>
                        <div className="mt-1 text-xs leading-snug text-slate-500">{b.proofLabel}</div>
                        {method && <MethodLink section={method} className="sm:justify-end" />}
                      </div>
                    </div>
                  </SpotlightCard>
                </Reveal>
              </li>
              );
            })}
          </ul>
        </div>
      </Container>
    </section>
  );
}
