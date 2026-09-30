import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from 'react';
import { AppWindow, MousePointerClick, Terminal } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useInView, usePrefersReducedMotion } from '@/lib/hooks';
import { VERSION } from '@/lib/site';
import { Accent, Container, Reveal, SectionHeading } from '../ui';
import { Cursor, Prompt, TerminalWindow, colorizeCommand } from '../terminal';
import { DesktopMock, ExplorerMock } from './ShowcaseMocks';

/* ------------------------------------------------------------------ */
/*  CLI panel                                                          */
/* ------------------------------------------------------------------ */
type CliCommand = { id: string; alias?: string; blurb: string; cmd: string; out: ReactNode[] };

const COMMANDS: CliCommand[] = [
  {
    id: 'create',
    alias: 'c',
    blurb: 'Pack files and folders',
    cmd: 'arca create arca.zip arca',
    out: [
      <>
        <span className="text-white">arca.zip</span>: 174 files, 2.3 MB <span className="text-slate-500">-&gt;</span>{' '}
        <span className="text-brand-300">898.5 KB</span> <span className="text-emerald-400">(61.2% smaller)</span> in 0.008 s{' '}
        <span className="text-slate-600">·</span> 288 MB/s <span className="text-slate-600">·</span> 24 threads
      </>,
    ],
  },
  {
    id: 'list',
    alias: 'l',
    blurb: 'See inside without extracting',
    cmd: 'arca list arca.zip --time',
    out: [
      <span className="whitespace-pre text-slate-400">{'        3453  deflate   62.6%  arca/.github/workflows/ci.yml'}</span>,
      <span className="whitespace-pre text-slate-400">{'        1250  deflate   58.7%  arca/.github/workflows/pages.yml'}</span>,
      <span className="whitespace-pre text-slate-400">{'        8287  deflate   63.8%  arca/.github/workflows/release.yml'}</span>,
      <span className="whitespace-pre text-slate-400">{'         192  deflate   27.6%  arca/.gitignore'}</span>,
      <span className="text-slate-600">…</span>,
      <span className="text-slate-500">
        174 entries, 2.3 MB uncompressed, listed in <span className="text-brand-300">0.2 ms</span>
      </span>,
    ],
  },
  {
    id: 'extract',
    alias: 'x',
    blurb: 'Unpack in parallel, safely',
    cmd: 'arca extract arca.zip -o build --on-conflict rename',
    out: [
      <>
        174 files, <span className="text-brand-300">2.3 MB</span> written in <span className="text-brand-300">0.009 s</span>
      </>,
    ],
  },
  {
    id: 'test',
    alias: 't',
    blurb: 'Verify every CRC, write nothing',
    cmd: 'arca test arca.zip',
    out: [
      <>
        <span className="text-emerald-400">174 entries verified, no errors</span> (0.006 s)
      </>,
    ],
  },
  {
    id: 'password',
    blurb: 'Add, change or remove a password',
    cmd: 'arca password arca.zip --new "correct horse"',
    out: [
      <>
        <span className="text-white">arca.zip</span>: 174 entries, 2.3 MB{' '}
        <span className="text-emerald-400">now encrypted with AES-256</span> in 0.072 s
      </>,
    ],
  },
  {
    id: 'bench',
    blurb: 'Measure the R2 requirement',
    cmd: 'arca bench list-6000.zip',
    out: [
      <span className="text-slate-400">Performance requirements (design document, section 05)</span>,
      <span>&nbsp;</span>,
      <span className="whitespace-pre">{'  R2  list without extracting'}</span>,
      <span className="whitespace-pre text-slate-400">{'      6000 entries in a 828.8 KB archive'}</span>,
      <span className="whitespace-pre">
        {'      '}
        <span className="text-brand-300">1.0 ms</span>
        {'   target < 200 ms   '}
        <span className="text-emerald-400">PASS</span>
      </span>,
      <span>&nbsp;</span>,
      <span className="whitespace-pre">{'  R1  cold start: measured from outside, with hyperfine'}</span>,
      <span className="whitespace-pre text-slate-400">{"      hyperfine --warmup 20 'arca --version'"}</span>,
    ],
  },
];

function CliPanel() {
  const reduced = usePrefersReducedMotion();
  const [ref, inView] = useInView<HTMLDivElement>();
  const [sel, setSel] = useState(0);
  const [typed, setTyped] = useState('');
  const [shown, setShown] = useState(0);
  const c = COMMANDS[sel];

  useEffect(() => {
    const cmd = COMMANDS[sel];
    if (reduced) {
      setTyped(cmd.cmd);
      setShown(cmd.out.length);
      return;
    }
    if (!inView) return;
    let t = 0;
    let i = 0;
    setTyped('');
    setShown(0);
    const showNext = (j: number) => {
      setShown(j);
      if (j < cmd.out.length) t = window.setTimeout(() => showNext(j + 1), 70);
    };
    const typeNext = () => {
      i++;
      setTyped(cmd.cmd.slice(0, i));
      if (i < cmd.cmd.length) t = window.setTimeout(typeNext, 14 + Math.random() * 26);
      else t = window.setTimeout(() => showNext(1), 320);
    };
    t = window.setTimeout(typeNext, 160);
    return () => window.clearTimeout(t);
  }, [sel, inView, reduced]);

  const done = shown >= c.out.length;

  return (
    <div ref={ref}>
      <div className="grid gap-3 lg:grid-cols-[260px_minmax(0,1fr)]">
        <div role="group" aria-label="Commands" className="no-scrollbar -mx-1 flex gap-2 overflow-x-auto px-1 pb-1 lg:mx-0 lg:flex-col lg:overflow-visible lg:px-0 lg:pb-0">
          {COMMANDS.map((cmd, i) => (
            <button
              key={cmd.id}
              type="button"
              aria-pressed={sel === i}
              onClick={() => setSel(i)}
              className={cn(
                'group relative shrink-0 rounded-2xl border px-4 py-3 text-left transition-all duration-300 lg:w-full',
                sel === i
                  ? 'border-brand-400/30 bg-brand-500/[0.08] shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]'
                  : 'border-white/[0.06] bg-white/[0.02] hover:border-white/15 hover:bg-white/[0.04]',
              )}
            >
              <span className="flex items-center gap-2 font-mono text-sm">
                <span className={sel === i ? 'text-brand-200' : 'text-slate-200'}>{cmd.id}</span>
                {cmd.alias && (
                  <span className="rounded bg-white/[0.07] px-1.5 text-[10px] text-slate-500">{cmd.alias}</span>
                )}
              </span>
              <span className="mt-0.5 hidden text-xs text-slate-500 sm:block">{cmd.blurb}</span>
            </button>
          ))}
        </div>

        <TerminalWindow title="site — arca — zsh" label={`Terminal running arca ${c.id}`} bodyClassName="min-h-[280px] sm:min-h-[320px]">
          <div className="break-all">
            <Prompt dir="demo" />
            {colorizeCommand(typed)}
            {shown === 0 && <Cursor />}
          </div>
          {c.out.slice(0, shown).map((line, i) => (
            <div key={`${c.id}-${i}`} className="animate-fade-in break-words">
              {line}
            </div>
          ))}
          {done && (
            <div className="mt-1">
              <Prompt dir="demo" />
              <Cursor />
            </div>
          )}
        </TerminalWindow>
      </div>
      <p className="mt-3 px-1 text-xs text-slate-600">
        Real <span className="font-mono">arca {VERSION}</span> output on a copy of this repository, Ryzen 9 5900X, Linux.
      </p>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Tabs                                                               */
/* ------------------------------------------------------------------ */
const TABS = [
  { id: 'cli', label: 'Terminal', icon: Terminal, caption: 'Six commands, four one-letter aliases, one line of output. Scripts and CI love it.' },
  { id: 'desktop', label: 'Desktop app', icon: AppWindow, caption: 'Try it: click the list, then use ↑ ↓, Space, Ctrl+A, the filter box or Extract all.' },
  { id: 'explorer', label: 'Explorer', icon: MousePointerClick, caption: 'Windows 11’s modern menu and the classic one — right-click, extract, done. Try it.' },
] as const;

type TabId = (typeof TABS)[number]['id'];

export function Showcase() {
  const [tab, setTab] = useState<TabId>('cli');
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [indicator, setIndicator] = useState({ left: 0, width: 0 });

  useLayoutEffect(() => {
    const update = () => {
      const i = TABS.findIndex((t) => t.id === tab);
      const el = tabRefs.current[i];
      if (el) setIndicator({ left: el.offsetLeft, width: el.offsetWidth });
    };
    update();
    document.fonts?.ready.then(update).catch(() => undefined);
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, [tab]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const idx = TABS.findIndex((t) => t.id === tab);
    let next = idx;
    if (e.key === 'ArrowRight') next = (idx + 1) % TABS.length;
    else if (e.key === 'ArrowLeft') next = (idx - 1 + TABS.length) % TABS.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = TABS.length - 1;
    else return;
    e.preventDefault();
    setTab(TABS[next].id);
    tabRefs.current[next]?.focus();
  };

  const activeTab = TABS.find((t) => t.id === tab) ?? TABS[0];

  return (
    <section id="showcase" aria-labelledby="showcase-title" className="relative py-24 sm:py-32">
      <Container>
        <SectionHeading
          eyebrow="Product"
          title={
            <span id="showcase-title">
              One engine. <Accent>Three</Accent> ways in.
            </span>
          }
          description="Script it from the terminal, browse it in a native window, or never leave File Explorer. Same core, same speed, same safety."
        />

        <Reveal delay={100}>
          <div className="mt-12 flex justify-center">
            <div
              role="tablist"
              aria-label="Ways to use Arca"
              onKeyDown={onKeyDown}
              className="glass relative inline-flex max-w-full rounded-full p-1.5"
            >
              <span
                aria-hidden="true"
                className="absolute inset-y-1.5 rounded-full bg-white shadow-[0_6px_20px_-6px_rgba(255,255,255,0.5)] transition-all duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]"
                style={{ left: indicator.left, width: indicator.width }}
              />
              {TABS.map((t, i) => (
                <button
                  key={t.id}
                  ref={(el) => {
                    tabRefs.current[i] = el;
                  }}
                  type="button"
                  role="tab"
                  id={`tab-${t.id}`}
                  aria-selected={tab === t.id}
                  aria-controls={`panel-${t.id}`}
                  tabIndex={tab === t.id ? 0 : -1}
                  onClick={() => setTab(t.id)}
                  className={cn(
                    'relative z-10 inline-flex h-10 items-center gap-2 rounded-full px-3.5 text-[13px] font-medium transition-colors duration-300 sm:px-5 sm:text-sm',
                    tab === t.id ? 'text-ink-950' : 'text-slate-400 hover:text-white',
                  )}
                >
                  <t.icon className="hidden size-4 sm:block" />
                  {t.label}
                </button>
              ))}
            </div>
          </div>
          <p key={activeTab.id} className="mx-auto mt-5 max-w-xl animate-fade-in text-center text-sm text-slate-500">
            {activeTab.caption}
          </p>
        </Reveal>

        <Reveal delay={180}>
          <div className="relative mt-10">
            <div
              aria-hidden="true"
              className="absolute -inset-x-6 -top-10 bottom-0 -z-10 rounded-[48px] bg-[radial-gradient(ellipse_at_top,rgba(47,107,255,0.13),transparent_65%)]"
            />
            <div className="rounded-[28px] border border-white/[0.07] bg-white/[0.02] p-2 shadow-[0_40px_120px_-40px_rgba(0,0,0,0.95)] sm:p-3">
              <div role="tabpanel" id="panel-cli" aria-labelledby="tab-cli" hidden={tab !== 'cli'} className="animate-fade-in">
                <CliPanel />
              </div>
              <div role="tabpanel" id="panel-desktop" aria-labelledby="tab-desktop" hidden={tab !== 'desktop'} className="animate-fade-in">
                <DesktopMock />
              </div>
              <div role="tabpanel" id="panel-explorer" aria-labelledby="tab-explorer" hidden={tab !== 'explorer'} className="animate-fade-in">
                <ExplorerMock onOpenInArca={() => setTab('desktop')} />
              </div>
            </div>
          </div>
        </Reveal>
      </Container>
    </section>
  );
}
