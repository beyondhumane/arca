import { useEffect, useRef, useState } from 'react';
import type {
  AnchorHTMLAttributes,
  CSSProperties,
  HTMLAttributes,
  PointerEvent as ReactPointerEvent,
  ReactNode,
} from 'react';
import { Check, Copy } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useInView } from '@/lib/hooks';
import { useCopy } from '@/lib/i18n';
import mark from '../../../brand/arca-monolito.svg';

/* ------------------------------------------------------------------ */
/*  Layout                                                             */
/* ------------------------------------------------------------------ */
export function Container({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cn('mx-auto w-full max-w-7xl px-5 sm:px-6 lg:px-8', className)}>{children}</div>;
}

export function GradientDivider({ className }: { className?: string }) {
  return (
    <div
      aria-hidden="true"
      className={cn('h-px w-full bg-linear-to-r from-transparent via-white/10 to-transparent', className)}
    />
  );
}

/* ------------------------------------------------------------------ */
/*  Scroll reveal                                                      */
/* ------------------------------------------------------------------ */
type RevealVariant = 'up' | 'left' | 'right' | 'scale' | 'fade';

export function Reveal({
  children,
  className,
  delay = 0,
  variant = 'up',
}: {
  children: ReactNode;
  className?: string;
  delay?: number;
  variant?: RevealVariant;
}) {
  const [ref, inView] = useInView<HTMLDivElement>();
  return (
    <div
      ref={ref}
      data-reveal={variant}
      data-visible={inView ? 'true' : 'false'}
      className={cn('reveal', className)}
      style={{ '--reveal-delay': `${delay}ms` } as CSSProperties}
    >
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Buttons                                                            */
/* ------------------------------------------------------------------ */
export type ButtonVariant = 'primary' | 'brand' | 'glass' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

const BUTTON_BASE =
  'btn group/btn relative isolate inline-flex select-none items-center justify-center gap-2 whitespace-nowrap rounded-full font-medium tracking-[-0.01em] transition-all duration-300 ease-out active:scale-[0.97] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-400/80 focus-visible:ring-offset-2 focus-visible:ring-offset-ink-950';

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary:
    'bg-white text-ink-950 shadow-[inset_0_1px_0_rgba(255,255,255,0.6),0_8px_28px_-10px_rgba(255,255,255,0.45)] hover:-translate-y-0.5 hover:shadow-[inset_0_1px_0_rgba(255,255,255,0.6),0_14px_38px_-10px_rgba(95,174,255,0.6)]',
  brand:
    'bg-[linear-gradient(110deg,#bfdbff_0%,#8cc4ff_30%,#3b9cff_68%,#ff8a3d_100%)] text-ink-950 shadow-[inset_0_1px_0_rgba(255,255,255,0.55),0_10px_32px_-10px_rgba(47,107,255,0.75)] hover:-translate-y-0.5 hover:shadow-[inset_0_1px_0_rgba(255,255,255,0.55),0_18px_46px_-10px_rgba(47,107,255,0.95)]',
  glass:
    'glass text-slate-100 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] hover:-translate-y-0.5 hover:border-white/20 hover:bg-white/[0.07]',
  ghost: 'text-slate-300 hover:text-white',
};

const BUTTON_SIZES: Record<ButtonSize, string> = {
  sm: 'h-9 px-4 text-[13px]',
  md: 'h-11 px-5 text-sm',
  lg: 'h-12 px-6 text-[15px]',
};

export function buttonClasses(variant: ButtonVariant = 'primary', size: ButtonSize = 'md', className?: string) {
  return cn(BUTTON_BASE, BUTTON_VARIANTS[variant], BUTTON_SIZES[size], className);
}

type ButtonLinkProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  external?: boolean;
};

export function ButtonLink({
  variant = 'primary',
  size = 'md',
  external,
  className,
  children,
  ...rest
}: ButtonLinkProps) {
  return (
    <a
      className={buttonClasses(variant, size, className)}
      {...(external ? { target: '_blank', rel: 'noreferrer noopener' } : {})}
      {...rest}
    >
      {(variant === 'primary' || variant === 'brand') && <span aria-hidden="true" className="btn-shine" />}
      <span className="relative z-10 inline-flex items-center gap-2">{children}</span>
    </a>
  );
}

/* ------------------------------------------------------------------ */
/*  Copy to clipboard                                                  */
/* ------------------------------------------------------------------ */
export function CopyButton({
  text,
  className,
  label,
}: {
  text: string;
  className?: string;
  label?: string;
}) {
  const [status, setStatus] = useState<'idle' | 'copied' | 'failed'>('idle');
  const copied = status === 'copied';
  const t = useCopy({
    en: { copy: 'Copy to clipboard', copied: 'Copied', done: 'Copied to clipboard', failed: 'Could not copy' },
    es: { copy: 'Copiar al portapapeles', copied: 'Copiado', done: 'Copiado al portapapeles', failed: 'No se pudo copiar' },
  });
  label ??= t.copy;
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async () => {
    let ok = true;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try {
        ok = document.execCommand('copy');
      } catch {
        ok = false;
      }
      document.body.removeChild(ta);
    }
    setStatus(ok ? 'copied' : 'failed');
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setStatus('idle'), 1800);
  };

  return (
    <button
      type="button"
      onClick={copy}
      aria-label={copied ? t.copied : status === 'failed' ? t.failed : label}
      title={copied ? t.copied : status === 'failed' ? t.failed : label}
      className={cn(
        'relative inline-flex size-8 shrink-0 items-center justify-center rounded-lg text-slate-400 transition hover:bg-white/[0.07] hover:text-white active:scale-90',
        className,
      )}
    >
      <Copy
        className={cn(
          'absolute size-4 transition-all duration-300',
          copied ? 'rotate-12 scale-50 opacity-0' : 'scale-100 opacity-100',
        )}
      />
      <Check
        className={cn(
          'absolute size-4 text-emerald-400 transition-all duration-300',
          copied ? 'scale-100 opacity-100' : '-rotate-12 scale-50 opacity-0',
        )}
      />
      <span className="sr-only" aria-live="polite">
        {copied ? t.done : status === 'failed' ? t.failed : ''}
      </span>
    </button>
  );
}

/* ------------------------------------------------------------------ */
/*  Typography helpers                                                 */
/* ------------------------------------------------------------------ */
export function Eyebrow({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.03] px-3 py-1 font-mono text-[11px] font-medium uppercase tracking-[0.18em] text-brand-300',
        className,
      )}
    >
      <span aria-hidden="true" className="size-1.5 rounded-full bg-brand-400 shadow-[0_0_10px_2px_rgba(95,174,255,0.6)]" />
      {children}
    </span>
  );
}

export function Accent({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={cn('brand-text pr-[0.04em] font-display font-semibold', className)}>
      {children}
    </span>
  );
}

export function SectionHeading({
  eyebrow,
  title,
  description,
  align = 'center',
  className,
}: {
  eyebrow: string;
  title: ReactNode;
  description?: ReactNode;
  align?: 'center' | 'left';
  className?: string;
}) {
  return (
    <div className={cn('max-w-3xl', align === 'center' && 'mx-auto text-center', className)}>
      <Reveal>
        <Eyebrow>{eyebrow}</Eyebrow>
      </Reveal>
      <Reveal delay={90}>
        <h2 className="mt-5 text-balance text-[34px] font-semibold leading-[1.06] tracking-[-0.035em] text-white sm:text-5xl lg:text-[56px]">
          {title}
        </h2>
      </Reveal>
      {description && (
        <Reveal delay={180}>
          <p
            className={cn(
              'mt-5 text-pretty text-base leading-relaxed text-slate-400 sm:text-lg',
              align === 'center' && 'mx-auto max-w-2xl',
            )}
          >
            {description}
          </p>
        </Reveal>
      )}
    </div>
  );
}

export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        'inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-white/10 border-b-white/20 bg-white/[0.05] px-1.5 font-mono text-[11px] font-medium text-slate-200 shadow-[inset_0_-1px_0_rgba(255,255,255,0.08),0_1px_2px_rgba(0,0,0,0.4)]',
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/* ------------------------------------------------------------------ */
/*  Spotlight card                                                     */
/* ------------------------------------------------------------------ */
export function SpotlightCard({ className, children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  const ref = useRef<HTMLDivElement>(null);
  const onMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    el.style.setProperty('--mx', `${e.clientX - r.left}px`);
    el.style.setProperty('--my', `${e.clientY - r.top}px`);
  };
  return (
    <div
      ref={ref}
      onPointerMove={onMove}
      className={cn(
        'spotlight overflow-hidden rounded-3xl border border-white/[0.07] bg-linear-to-b from-white/[0.035] to-white/[0.01] shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]',
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Brand                                                              */
/* ------------------------------------------------------------------ */
export function LogoMark({ className }: { className?: string }) {
  return <img src={mark} alt="" aria-hidden="true" className={className} />;
}

export function Logo({ className, suffix }: { className?: string; suffix?: ReactNode }) {
  return (
    <span className={cn('flex items-center gap-2.5', className)}>
      <LogoMark className="h-6 w-auto drop-shadow-[0_4px_14px_rgba(47,107,255,0.35)]" />
      <span className="font-display text-[17px] font-semibold tracking-[-0.02em] text-white">Arca</span>
      {suffix}
    </span>
  );
}
