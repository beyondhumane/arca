import type { ComponentType, ReactNode, SVGProps } from 'react';
import { Info, Lightbulb, TriangleAlert } from 'lucide-react';
import { cn } from '@/utils/cn';
import { CopyButton, Kbd } from '../ui';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

/* ------------------------------------------------------------------ */
/*  Text                                                               */
/* ------------------------------------------------------------------ */
export function H2({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h2 id={id} className="mt-14 scroll-mt-28 text-2xl font-semibold tracking-[-0.025em] text-white first:mt-0">
      {children}
    </h2>
  );
}

export function H3({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h3 id={id} className="mt-10 scroll-mt-28 text-lg font-semibold tracking-[-0.015em] text-slate-100">
      {children}
    </h3>
  );
}

export function P({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={cn('mt-4 text-[15px] leading-7 text-slate-400', className)}>{children}</p>;
}

export function C({ children }: { children: ReactNode }) {
  return (
    <code className="rounded-md border border-white/10 bg-white/[0.045] px-1.5 py-0.5 font-mono text-[0.84em] text-brand-200">
      {children}
    </code>
  );
}

export function Strong({ children }: { children: ReactNode }) {
  return <strong className="font-medium text-slate-100">{children}</strong>;
}

export function A({ href, children }: { href: string; children: ReactNode }) {
  const external = href.startsWith('http');
  return (
    <a
      href={href}
      {...(external ? { target: '_blank', rel: 'noreferrer noopener' } : {})}
      className="text-brand-300 underline decoration-brand-400/30 underline-offset-4 transition hover:text-brand-200 hover:decoration-brand-300"
    >
      {children}
    </a>
  );
}

export function UL({ children }: { children: ReactNode }) {
  return <ul className="mt-4 space-y-2.5 text-[15px] leading-7 text-slate-400">{children}</ul>;
}

export function LI({ children }: { children: ReactNode }) {
  return (
    <li className="relative pl-6 before:absolute before:left-1 before:top-[0.72em] before:size-1.5 before:rounded-full before:bg-brand-400/70">
      {children}
    </li>
  );
}

export function Keys({ combo }: { combo: string }) {
  const parts = combo.split('+');
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap align-middle">
      {parts.map((k, i) => (
        <span key={`${k}-${i}`} className="inline-flex items-center gap-1">
          {i > 0 && <span className="text-xs text-slate-600">+</span>}
          <Kbd>{k}</Kbd>
        </span>
      ))}
    </span>
  );
}

/* ------------------------------------------------------------------ */
/*  Code                                                               */
/* ------------------------------------------------------------------ */
export type Lang = 'bash' | 'text' | 'yaml' | 'toml' | 'rust';

const LANG_LABEL: Record<Lang, string> = {
  bash: 'Terminal',
  text: 'Output',
  yaml: 'YAML',
  toml: 'TOML',
  rust: 'Rust',
};

function highlightShell(line: string): ReactNode {
  if (/^\s*#/.test(line)) return <span className="text-slate-500">{line}</span>;
  let code = line;
  let comment = '';
  const m = /\s+#\s.*$/.exec(line);
  if (m) {
    code = line.slice(0, m.index);
    comment = line.slice(m.index);
  }
  const tokens = code.split(/("[^"]*"|'[^']*'|\s+|&&|\|)/).filter((t) => t !== '');
  let expectCmd = true;
  const nodes = tokens.map((t, i) => {
    if (/^\s+$/.test(t)) return t;
    if (t === '&&' || t === '|') {
      expectCmd = true;
      return (
        <span key={i} className="text-slate-500">
          {t}
        </span>
      );
    }
    let cls = 'text-slate-200';
    if (expectCmd) {
      cls = t === 'arca' ? 'text-brand-300' : 'text-sky-300';
      expectCmd = t === 'sudo';
    } else if (/^["']/.test(t)) cls = 'text-emerald-300';
    else if (/^--?[A-Za-z]/.test(t)) cls = 'text-violet-300';
    else if (t === 'arca') cls = 'text-brand-300';
    return (
      <span key={i} className={cls}>
        {t}
      </span>
    );
  });
  return (
    <>
      {nodes}
      {comment && <span className="text-slate-500">{comment}</span>}
    </>
  );
}

function highlightYaml(line: string): ReactNode {
  if (/^\s*#/.test(line)) return <span className="text-slate-500">{line}</span>;
  const m = /^(\s*-?\s*)([\w.-]+)(:)(.*)$/.exec(line);
  if (!m) return <span className="text-slate-200">{line}</span>;
  return (
    <>
      {m[1]}
      <span className="text-sky-300">{m[2]}</span>
      <span className="text-slate-500">{m[3]}</span>
      <span className="text-emerald-300/90">{m[4]}</span>
    </>
  );
}

function highlightToml(line: string): ReactNode {
  if (/^\s*#/.test(line)) return <span className="text-slate-500">{line}</span>;
  if (/^\s*\[.*\]\s*$/.test(line)) return <span className="text-violet-300">{line}</span>;
  const m = /^(\s*[\w."*-]+)(\s*=\s*)(.*)$/.exec(line);
  if (!m) return <span className="text-slate-200">{line}</span>;
  return (
    <>
      <span className="text-sky-300">{m[1]}</span>
      <span className="text-slate-500">{m[2]}</span>
      <span className="text-emerald-300/90">{m[3]}</span>
    </>
  );
}

const RUST_KEYWORDS = /^(fn|let|mut|match|use|pub|mod|impl|struct|enum|if|else|return|for|in|while|loop|as|const|static|crate|self|Self|where|move|ref|dyn|true|false)$/;

function highlightRust(line: string): ReactNode {
  const commentAt = line.indexOf('//');
  const code = commentAt >= 0 ? line.slice(0, commentAt) : line;
  const comment = commentAt >= 0 ? line.slice(commentAt) : '';
  if (/^\s*#!?\[/.test(code)) {
    return (
      <>
        <span className="text-violet-300">{code}</span>
        {comment && <span className="text-slate-500">{comment}</span>}
      </>
    );
  }
  const tokens = code.split(/("[^"]*"|[A-Za-z_][A-Za-z0-9_]*)/).filter((t) => t !== '');
  return (
    <>
      {tokens.map((t, i) => {
        let cls = 'text-slate-300';
        if (t.startsWith('"')) cls = 'text-emerald-300';
        else if (RUST_KEYWORDS.test(t)) cls = 'text-spark-300';
        else if (/^(Ok|Err|Some|None)$/.test(t)) cls = 'text-brand-300';
        else if (/^[A-Z][A-Za-z0-9_]*$/.test(t)) cls = 'text-sky-300';
        return (
          <span key={i} className={cls}>
            {t}
          </span>
        );
      })}
      {comment && <span className="text-slate-500">{comment}</span>}
    </>
  );
}

function highlight(line: string, lang: Lang): ReactNode {
  switch (lang) {
    case 'bash':
      return highlightShell(line);
    case 'yaml':
      return highlightYaml(line);
    case 'toml':
      return highlightToml(line);
    case 'rust':
      return highlightRust(line);
    default:
      return <span className="text-slate-300">{line}</span>;
  }
}

export function CodeBlock({ code, lang = 'bash', title }: { code: string; lang?: Lang; title?: string }) {
  const clean = code.replace(/^\n+|\n+$/g, '');
  const lines = clean.split('\n');
  return (
    <div className="my-6 overflow-hidden rounded-2xl border border-white/[0.08] bg-ink-900/80 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      <div className="flex items-center justify-between border-b border-white/[0.06] bg-white/[0.02] py-1 pl-4 pr-1">
        <span className="font-mono text-[11px] text-slate-500">{title ?? LANG_LABEL[lang]}</span>
        <CopyButton text={clean} label={`Copy ${title ?? LANG_LABEL[lang].toLowerCase()}`} />
      </div>
      <pre className="thin-scrollbar overflow-x-auto px-4 py-4 font-mono text-[13px] leading-relaxed">
        <code>
          {lines.map((l, i) => (
            <span key={i} className="block min-h-[1.5em] whitespace-pre">
              {highlight(l, lang)}
            </span>
          ))}
        </code>
      </pre>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Tables                                                             */
/* ------------------------------------------------------------------ */
export function DocTable({ head, rows, minWidth = 520 }: { head: ReactNode[]; rows: ReactNode[][]; minWidth?: number }) {
  return (
    <div className="thin-scrollbar my-6 overflow-x-auto rounded-2xl border border-white/[0.08]">
      <table className="w-full border-collapse text-left text-sm" style={{ minWidth }}>
        <thead className="bg-white/[0.03]">
          <tr>
            {head.map((h, i) => (
              <th key={i} scope="col" className="border-b border-white/[0.08] px-4 py-3 font-medium text-slate-200">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i} className="border-b border-white/[0.05] transition-colors last:border-0 hover:bg-white/[0.02]">
              {r.map((c, j) => (
                <td key={j} className={cn('px-4 py-3 align-top leading-relaxed', j === 0 ? 'text-slate-200' : 'text-slate-400')}>
                  {c}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Callouts                                                           */
/* ------------------------------------------------------------------ */
const CALLOUTS: Record<'note' | 'tip' | 'warning', { icon: IconType; box: string; icn: string; label: string }> = {
  note: { icon: Info, box: 'border-sky-400/20 bg-sky-400/[0.05]', icn: 'text-sky-300', label: 'Note' },
  tip: { icon: Lightbulb, box: 'border-emerald-400/20 bg-emerald-400/[0.05]', icn: 'text-emerald-300', label: 'Tip' },
  warning: { icon: TriangleAlert, box: 'border-amber-400/25 bg-amber-400/[0.05]', icn: 'text-amber-300', label: 'Heads up' },
};

export function Callout({ type = 'note', title, children }: { type?: 'note' | 'tip' | 'warning'; title?: string; children: ReactNode }) {
  const c = CALLOUTS[type];
  return (
    <aside className={cn('my-6 flex gap-3 rounded-2xl border p-4 sm:p-5', c.box)} aria-label={title ?? c.label}>
      <c.icon className={cn('mt-0.5 size-5 shrink-0', c.icn)} />
      <div className="min-w-0 text-[15px] leading-7 text-slate-300">
        <p className="font-medium text-white">{title ?? c.label}</p>
        <div className="mt-1 text-slate-400">{children}</div>
      </div>
    </aside>
  );
}

/* ------------------------------------------------------------------ */
/*  Steps and cards                                                    */
/* ------------------------------------------------------------------ */
export function Steps({ children }: { children: ReactNode }) {
  return <ol className="relative mt-8 space-y-10 border-l border-white/[0.08] pl-8 [counter-reset:step]">{children}</ol>;
}

export function Step({ title, children }: { title: string; children: ReactNode }) {
  return (
    <li className="relative [counter-increment:step] before:absolute before:-left-[47px] before:top-0 before:flex before:size-7 before:items-center before:justify-center before:rounded-full before:border before:border-brand-400/30 before:bg-ink-850 before:font-mono before:text-xs before:text-brand-300 before:content-[counter(step)]">
      <h3 className="text-base font-semibold leading-7 text-white">{title}</h3>
      <div className="text-[15px] text-slate-400">{children}</div>
    </li>
  );
}
