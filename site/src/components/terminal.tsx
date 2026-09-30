import type { ReactNode, Ref } from 'react';
import { Terminal as TerminalIcon } from 'lucide-react';
import { cn } from '@/utils/cn';

/** Colours a shell command: binary, subcommand, flags and quoted strings. */
export function colorizeCommand(text: string): ReactNode[] {
  const parts = text.split(/("[^"]*"?|\s+)/).filter((p) => p !== '');
  let seenBinary = false;
  let seenSub = false;
  return parts.map((p, i) => {
    if (/^\s+$/.test(p)) return p;
    let cls = 'text-slate-100';
    if (!seenBinary) {
      seenBinary = true;
      cls = p === 'arca' ? 'text-brand-300' : 'text-sky-300';
    } else if (!seenSub && /^[a-z]+$/.test(p)) {
      seenSub = true;
      cls = 'text-sky-300';
    } else if (/^-{1,2}[\w-]+/.test(p)) {
      cls = 'text-violet-300';
    } else if (p.startsWith('"')) {
      cls = 'text-emerald-300';
    }
    return (
      <span key={i} className={cls}>
        {p}
      </span>
    );
  });
}

export function Cursor({ className }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'ml-px inline-block h-[1.15em] w-[0.55em] translate-y-[0.2em] animate-blink rounded-[1px] bg-brand-300/90',
        className,
      )}
    />
  );
}

export function Prompt({ dir = 'app' }: { dir?: string }) {
  return (
    <span className="select-none">
      <span className="text-emerald-400">➜</span> <span className="text-sky-300">{dir}</span>{' '}
      <span className="text-slate-500">git:(</span>
      <span className="text-rose-300">main</span>
      <span className="text-slate-500">)</span>{' '}
    </span>
  );
}

export function TerminalWindow({
  title = 'arca — zsh',
  children,
  className,
  bodyClassName,
  bodyRef,
  label,
}: {
  title?: string;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  bodyRef?: Ref<HTMLDivElement>;
  label?: string;
}) {
  return (
    <div
      role="region"
      aria-label={label ?? 'Terminal demonstration'}
      className={cn(
        'relative overflow-hidden rounded-2xl border border-white/10 bg-ink-900/85 shadow-[0_40px_120px_-30px_rgba(0,0,0,0.95),inset_0_1px_0_rgba(255,255,255,0.05)] backdrop-blur-xl',
        className,
      )}
    >
      <div className="flex items-center gap-3 border-b border-white/[0.06] bg-white/[0.025] px-4 py-3">
        <div className="flex gap-2" aria-hidden="true">
          <span className="size-3 rounded-full bg-[#ff5f57]/90" />
          <span className="size-3 rounded-full bg-[#febc2e]/90" />
          <span className="size-3 rounded-full bg-[#28c840]/90" />
        </div>
        <div className="mx-auto flex items-center gap-2 font-mono text-xs text-slate-500">
          <TerminalIcon className="size-3.5" />
          {title}
        </div>
        <div className="w-[52px]" aria-hidden="true" />
      </div>
      <div
        ref={bodyRef}
        className={cn('px-4 py-4 font-mono text-[11.5px] leading-[1.8] text-slate-300 sm:px-6 sm:text-[13px]', bodyClassName)}
      >
        {children}
      </div>
    </div>
  );
}
