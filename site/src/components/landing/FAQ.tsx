import { useId, useState } from 'react';
import type { ReactNode } from 'react';
import { ArrowUpRight, Plus } from 'lucide-react';
import { cn } from '@/utils/cn';
import { NEW_ISSUE_URL } from '@/lib/site';
import { useCopy, useLang } from '@/lib/i18n';
import { Accent, Container, Eyebrow, Reveal } from '../ui';
import { docPath } from '@/lib/router';

const Code = ({ children }: { children: ReactNode }) => (
  <code className="rounded-md border border-white/10 bg-white/[0.04] px-1.5 py-0.5 font-mono text-[0.85em] text-brand-200">{children}</code>
);

const FAQS_EN: { q: string; a: ReactNode }[] = [
  {
    q: 'Is Arca free to use at work?',
    a: (
      <>
        Yes. Arca is released under the Apache License 2.0: use it at home or at work, modify it, and ship it inside
        your own products. The license also includes an explicit patent grant. There is no paid edition.
      </>
    ),
  },
  {
    q: 'Which formats does Arca support?',
    a: (
      <>
        ZIP (including Zip64) with Store, Deflate (via zlib-rs) and Zstandard; ustar TAR with checksum verification;
        and <Code>.tar.gz</Code> / <Code>.tgz</Code>. Legacy ZipCrypto archives can be opened. The 7z format, xz/LZMA2,
        symbolic links, GNU tar long names and solid archives are on the roadmap.
      </>
    ),
  },
  {
    q: 'Can other tools open the archives Arca creates?',
    a: (
      <>
        Yes. <Code>interop.sh</Code> checks it across 35 cases verified by SHA-256: unzip, tar and 7-Zip read
        what Arca writes at all four levels, and Arca reads what they write. One caveat: Zstandard inside ZIP (method 93)
        isn’t read by classic unzip, which is why <Code>-c auto</Code> uses Deflate for <Code>.zip</Code>.
      </>
    ),
  },
  {
    q: 'How strong is the encryption?',
    a: (
      <>
        AES-256 using the WinZip AE-2 scheme: PBKDF2-HMAC-SHA1 key derivation, AES-256 in CTR mode, an HMAC-SHA1 over
        the ciphertext and a random 16-byte salt per entry, the same scheme 7-Zip, WinRAR and NanaZip write. ZIP
        never encrypts file names, so the listing stays visible without the password.
      </>
    ),
  },
  {
    q: 'Is Arca always faster than 7-Zip?',
    a: (
      <>
        No, and we say so. With large files Arca compressed 5.7× faster at a 3.7% size cost (deflate vs deflate, Windows
        11). With thousands of small files 7-Zip is slightly ahead at the same size, because file creation on NTFS
        dominates. The <a href={docPath('en', 'benchmarks', 'deflate-against-deflate-on-windows-11')} className="text-brand-300 underline decoration-brand-400/40 underline-offset-4 hover:text-brand-200">benchmarks page</a> shows both.
      </>
    ),
  },
  {
    q: 'Which platforms are supported?',
    a: (
      <>
        Windows, macOS and Linux, with prebuilt binaries for Windows x86_64 (installer), macOS arm64 and x86_64, and
        Linux x86_64. The Explorer context menu is Windows 11 only for now; desktop integration on Linux and macOS is on
        the roadmap.
      </>
    ),
  },
  {
    q: 'Can I use Arca as a Rust library?',
    a: (
      <>
        The crates (<Code>arca-core</Code>, <Code>arca-zip</Code>, <Code>arca-tar</Code>) aren’t on crates.io yet. You
        can depend on them straight from the Git repository.
      </>
    ),
  },
  {
    q: 'How do I report a bug or contribute?',
    a: (
      <>
        Open an issue on GitHub with <Code>arca --version</Code>, your OS and the command you ran. Pull requests,
        benchmarks from your own hardware and roadmap help are all welcome.
      </>
    ),
  },
];

const FAQS_ES: typeof FAQS_EN = [
  {
    q: '¿Puedo usar Arca en el trabajo sin pagar?',
    a: (
      <>
        Sí. Arca se publica bajo la licencia Apache 2.0: úsalo en casa o en el trabajo, modifícalo y distribúyelo dentro
        de tus propios productos. La licencia incluye además una cesión explícita de patentes. No hay edición de pago.
      </>
    ),
  },
  {
    q: '¿Qué formatos admite Arca?',
    a: (
      <>
        ZIP (con Zip64) con Store, Deflate (sobre zlib-rs) y Zstandard; TAR ustar con verificación de la suma de control;
        y <Code>.tar.gz</Code> / <Code>.tgz</Code>. Los archivos con el antiguo ZipCrypto se pueden abrir. El formato 7z,
        xz/LZMA2, los enlaces simbólicos, los nombres largos de GNU tar y los archivos sólidos están en la hoja de ruta.
      </>
    ),
  },
  {
    q: '¿Otras herramientas abren los archivos que crea Arca?',
    a: (
      <>
        Sí. <Code>interop.sh</Code> lo comprueba en 35 casos verificados por SHA-256: unzip, tar y 7-Zip leen lo que
        escribe Arca en los cuatro niveles, y Arca lee lo que escriben ellos. Una salvedad: el unzip clásico no lee
        Zstandard dentro de ZIP (método 93), y por eso <Code>-c auto</Code> usa Deflate para <Code>.zip</Code>.
      </>
    ),
  },
  {
    q: '¿Cómo de fuerte es el cifrado?',
    a: (
      <>
        AES-256 con el esquema WinZip AE-2: derivación de clave PBKDF2-HMAC-SHA1, AES-256 en modo CTR, un HMAC-SHA1 sobre
        el texto cifrado y una sal aleatoria de 16 bytes por entrada, el mismo esquema que escriben 7-Zip, WinRAR y
        NanaZip. ZIP nunca cifra los nombres de los archivos, así que el listado se ve sin la contraseña.
      </>
    ),
  },
  {
    q: '¿Arca es siempre más rápido que 7-Zip?',
    a: (
      <>
        No, y lo decimos. Con archivos grandes Arca comprimió 5,7× más rápido a cambio de un 3,7% más de tamaño (deflate
        contra deflate, Windows 11). Con miles de archivos pequeños 7-Zip va algo por delante con el mismo tamaño, porque
        domina el tiempo de crear archivos en NTFS. La{' '}
        <a href={docPath('es', 'benchmarks', 'deflate-against-deflate-on-windows-11')} className="text-brand-300 underline decoration-brand-400/40 underline-offset-4 hover:text-brand-200">página de benchmarks</a> muestra los dos casos.
      </>
    ),
  },
  {
    q: '¿Qué plataformas se admiten?',
    a: (
      <>
        Windows, macOS y Linux, con binarios para Windows x86_64 (instalador), macOS arm64 y x86_64, y Linux x86_64. El
        menú contextual del Explorador solo existe en Windows 11 por ahora; la integración con el escritorio en Linux y
        macOS está en la hoja de ruta.
      </>
    ),
  },
  {
    q: '¿Puedo usar Arca como biblioteca de Rust?',
    a: (
      <>
        Los crates (<Code>arca-core</Code>, <Code>arca-zip</Code>, <Code>arca-tar</Code>) aún no están en crates.io.
        Puedes depender de ellos directamente desde el repositorio Git.
      </>
    ),
  },
  {
    q: '¿Cómo informo de un fallo o colaboro?',
    a: (
      <>
        Abre un issue en GitHub con la salida de <Code>arca --version</Code>, tu sistema operativo y el comando que
        ejecutaste. Los pull requests, los benchmarks en tu propio hardware y la ayuda con la hoja de ruta son bienvenidos.
      </>
    ),
  },
];

export const FAQS = { en: FAQS_EN, es: FAQS_ES };

const COPY = {
  en: {
    faqs: FAQS_EN,
    title: ['Questions, ', 'answered.'],
    lead: 'Short answers. The docs have the long ones.',
    browse: 'Browse the documentation',
    ask: 'Ask on GitHub Issues',
  },
  es: {
    faqs: FAQS_ES,
    title: ['Preguntas ', 'frecuentes.'],
    lead: 'Respuestas cortas. La documentación tiene las largas.',
    browse: 'Ver la documentación',
    ask: 'Preguntar en GitHub Issues',
  },
};

function FaqItem({ q, a, open, onToggle }: { q: string; a: ReactNode; open: boolean; onToggle: () => void }) {
  const id = useId();
  const btnId = `${id}-q`;
  const panelId = `${id}-a`;
  return (
    <div className={cn('rounded-2xl border transition-colors duration-300', open ? 'border-white/[0.12] bg-white/[0.035]' : 'border-white/[0.06] bg-white/[0.015] hover:border-white/10')}>
      <h3>
        <button
          id={btnId}
          type="button"
          aria-expanded={open}
          aria-controls={panelId}
          onClick={onToggle}
          className="flex w-full items-center justify-between gap-6 rounded-2xl px-5 py-5 text-left sm:px-6"
        >
          <span className={cn('text-[15px] font-medium transition-colors sm:text-base', open ? 'text-white' : 'text-slate-200')}>{q}</span>
          <span
            className={cn(
              'flex size-8 shrink-0 items-center justify-center rounded-full border transition-all duration-500',
              open ? 'rotate-45 border-brand-400/40 bg-brand-500/10 text-brand-300' : 'border-white/10 text-slate-400',
            )}
          >
            <Plus className="size-4" />
          </span>
        </button>
      </h3>
      <div
        id={panelId}
        role="region"
        aria-labelledby={btnId}
        className={cn('grid transition-[grid-template-rows] duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]', open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]')}
      >
        <div className="overflow-hidden">
          <div
            className={cn('px-5 pb-6 text-[15px] leading-relaxed text-slate-400 transition-opacity duration-500 sm:px-6', open ? 'opacity-100' : 'opacity-0')}
            {...(!open ? { inert: true } : {})}
          >
            {a}
          </div>
        </div>
      </div>
    </div>
  );
}

export function FAQ() {
  const { lang } = useLang();
  const [open, setOpen] = useState<number | null>(0);
  const t = useCopy(COPY);
  return (
    <section id="faq" aria-labelledby="faq-title" className="relative py-24 sm:py-32">
      <Container>
        <div className="grid gap-12 lg:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)] lg:gap-16">
          <div className="lg:sticky lg:top-28 lg:self-start">
            <Reveal>
              <Eyebrow>FAQ</Eyebrow>
            </Reveal>
            <Reveal delay={90}>
              <h2 id="faq-title" className="mt-5 text-balance text-[34px] font-semibold leading-[1.06] tracking-[-0.035em] text-white sm:text-5xl">
                {t.title[0]}<Accent>{t.title[1]}</Accent>
              </h2>
            </Reveal>
            <Reveal delay={180}>
              <p className="mt-5 max-w-sm text-base leading-relaxed text-slate-400">
                {t.lead}
              </p>
            </Reveal>
            <Reveal delay={260}>
              <div className="mt-8 flex flex-col gap-2 text-sm">
                <a href={docPath(lang)} className="inline-flex items-center gap-1.5 text-brand-300 transition hover:gap-2.5 hover:text-brand-200">
                  {t.browse} <ArrowUpRight className="size-4" />
                </a>
                <a href={NEW_ISSUE_URL} target="_blank" rel="noreferrer noopener" className="inline-flex items-center gap-1.5 text-slate-400 transition hover:gap-2.5 hover:text-white">
                  {t.ask} <ArrowUpRight className="size-4" />
                </a>
              </div>
            </Reveal>
          </div>

          <div className="space-y-3">
            {t.faqs.map((f, i) => (
              <Reveal key={i} delay={Math.min(i, 5) * 60}>
                <FaqItem q={f.q} a={f.a} open={open === i} onToggle={() => setOpen((o) => (o === i ? null : i))} />
              </Reveal>
            ))}
          </div>
        </div>
      </Container>
    </section>
  );
}
