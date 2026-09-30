import { useEffect, useMemo, useRef, useState } from 'react';
import type { ComponentType, KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent, SVGProps } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Braces,
  Check,
  ChevronRight,
  Copy,
  Download,
  FileArchive,
  FileImage,
  FileText,
  Film,
  Folder,
  FolderOpen,
  KeyRound,
  Lock,
  Monitor,
  Pencil,
  Scissors,
  Search,
  Settings2,
  Share2,
  Type,
  X,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { Kbd, LogoMark } from '../ui';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

/* ================================================================== */
/*  Desktop window                                                     */
/* ================================================================== */
type Kind = 'folder' | 'image' | 'pdf' | 'video' | 'doc' | 'font' | 'data';

const KIND_ICON: Record<Kind, { icon: IconType; cls: string }> = {
  folder: { icon: Folder, cls: 'text-amber-300 fill-amber-300/20' },
  image: { icon: FileImage, cls: 'text-sky-300' },
  pdf: { icon: FileText, cls: 'text-rose-300' },
  video: { icon: Film, cls: 'text-violet-300' },
  doc: { icon: FileText, cls: 'text-slate-300' },
  font: { icon: Type, cls: 'text-teal-300' },
  data: { icon: Braces, cls: 'text-lime-300' },
};

type Entry = { name: string; kind: Kind; size: string; packed: string; method: string; saved: string; modified: string };

const ENTRIES: Entry[] = [
  { name: 'icons', kind: 'folder', size: '—', packed: '—', method: '—', saved: '—', modified: '2026-09-14 09:12' },
  { name: 'exports', kind: 'folder', size: '—', packed: '—', method: '—', saved: '—', modified: '2026-09-13 17:31' },
  { name: 'brand-guidelines.pdf', kind: 'pdf', size: '12.4 MB', packed: '10.1 MB', method: 'deflate', saved: '19%', modified: '2026-09-12 16:40' },
  { name: 'hero@2x.png', kind: 'image', size: '4.8 MB', packed: '4.7 MB', method: 'deflate', saved: '2%', modified: '2026-09-12 14:03' },
  { name: 'logo.svg', kind: 'image', size: '18.2 KB', packed: '5.1 KB', method: 'deflate', saved: '72%', modified: '2026-09-11 11:27' },
  { name: 'palette.json', kind: 'data', size: '2.4 KB', packed: '0.8 KB', method: 'deflate', saved: '67%', modified: '2026-09-10 18:55' },
  { name: 'notes.md', kind: 'doc', size: '6.1 KB', packed: '2.3 KB', method: 'deflate', saved: '62%', modified: '2026-09-10 10:02' },
  { name: 'teaser.mp4', kind: 'video', size: '88.3 MB', packed: '88.3 MB', method: 'store', saved: '0%', modified: '2026-09-09 21:18' },
  { name: 'Geist-Variable.woff2', kind: 'font', size: '64.0 KB', packed: '63.7 KB', method: 'store', saved: '0%', modified: '2026-09-08 08:44' },
];

const GRID_COLS =
  'grid-cols-[28px_minmax(0,1fr)_72px] sm:grid-cols-[28px_minmax(0,1fr)_76px_76px_68px_52px] lg:grid-cols-[28px_minmax(0,1fr)_84px_84px_72px_56px_136px]';

type Toast = { text: string; progress: number | null } | null;

function useToast() {
  const [toast, setToast] = useState<Toast>(null);
  const timers = useRef<number[]>([]);
  const clear = () => {
    timers.current.forEach((t) => window.clearTimeout(t));
    timers.current.forEach((t) => window.clearInterval(t));
    timers.current = [];
  };
  useEffect(() => clear, []);

  const flash = (text: string, ms = 1900) => {
    clear();
    setToast({ text, progress: null });
    timers.current.push(window.setTimeout(() => setToast(null), ms));
  };

  const run = (text: string, doneText: string, duration = 1400) => {
    clear();
    const start = performance.now();
    setToast({ text, progress: 0 });
    const id = window.setInterval(() => {
      const p = Math.min(100, ((performance.now() - start) / duration) * 100);
      setToast({ text, progress: p });
      if (p >= 100) {
        window.clearInterval(id);
        setToast({ text: doneText, progress: null });
        timers.current.push(window.setTimeout(() => setToast(null), 2000));
      }
    }, 40);
    timers.current.push(id);
  };

  return { toast, flash, run };
}

export function DesktopMock() {
  const [filter, setFilter] = useState('');
  const [cursor, setCursor] = useState(2);
  const [ticked, setTicked] = useState<Set<string>>(() => new Set(['brand-guidelines.pdf', 'logo.svg']));
  const [encrypted, setEncrypted] = useState(false);
  const anchor = useRef(2);
  const { toast, flash, run } = useToast();

  const rows = useMemo(
    () => ENTRIES.filter((e) => e.name.toLowerCase().includes(filter.trim().toLowerCase())),
    [filter],
  );
  const safeCursor = rows.length === 0 ? -1 : Math.min(cursor, rows.length - 1);

  const toggle = (name: string) =>
    setTicked((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

  const open = (e: Entry) => {
    if (e.kind === 'folder') flash(`Opened ${e.name}/. Backspace goes up.`);
    else flash(`Extracted ${e.name} to a temp folder and opened it`);
  };

  const onKeyDown = (ev: ReactKeyboardEvent<HTMLDivElement>) => {
    if (rows.length === 0) return;
    const last = rows.length - 1;
    const move = (to: number, extend = false) => {
      const next = Math.max(0, Math.min(last, to));
      if (extend) {
        const from = Math.min(anchor.current, next);
        const toIdx = Math.max(anchor.current, next);
        setTicked((prev) => {
          const s = new Set(prev);
          for (let i = from; i <= toIdx; i++) s.add(rows[i].name);
          return s;
        });
      } else {
        anchor.current = next;
      }
      setCursor(next);
    };
    switch (ev.key) {
      case 'ArrowDown':
        ev.preventDefault();
        move(safeCursor + 1, ev.shiftKey);
        break;
      case 'ArrowUp':
        ev.preventDefault();
        move(safeCursor - 1, ev.shiftKey);
        break;
      case 'Home':
        ev.preventDefault();
        move(0);
        break;
      case 'End':
        ev.preventDefault();
        move(last);
        break;
      case 'PageDown':
        ev.preventDefault();
        move(safeCursor + 5);
        break;
      case 'PageUp':
        ev.preventDefault();
        move(safeCursor - 5);
        break;
      case ' ':
        ev.preventDefault();
        toggle(rows[safeCursor].name);
        break;
      case 'Enter':
        ev.preventDefault();
        open(rows[safeCursor]);
        break;
      case 'Backspace':
        ev.preventDefault();
        flash('Already at the top of project-assets.zip');
        break;
      default:
        if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'a') {
          ev.preventDefault();
          const all = rows.every((r) => ticked.has(r.name));
          setTicked(all ? new Set() : new Set(rows.map((r) => r.name)));
        } else if (ev.key.length === 1 && /[a-z0-9]/i.test(ev.key) && !ev.ctrlKey && !ev.metaKey) {
          const k = ev.key.toLowerCase();
          for (let step = 1; step <= rows.length; step++) {
            const idx = (safeCursor + step) % rows.length;
            if (rows[idx].name.toLowerCase().startsWith(k)) {
              move(idx);
              break;
            }
          }
        }
    }
  };

  const onRowClick = (ev: ReactMouseEvent<HTMLDivElement>, idx: number) => {
    const name = rows[idx].name;
    if (ev.shiftKey) {
      const from = Math.min(anchor.current, idx);
      const to = Math.max(anchor.current, idx);
      setTicked((prev) => {
        const s = new Set(prev);
        for (let i = from; i <= to; i++) s.add(rows[i].name);
        return s;
      });
    } else if (ev.ctrlKey || ev.metaKey) {
      toggle(name);
      anchor.current = idx;
    } else {
      anchor.current = idx;
    }
    setCursor(idx);
  };

  const activeId = safeCursor >= 0 ? `arca-row-${safeCursor}` : undefined;

  return (
    <div className="relative overflow-hidden rounded-2xl border border-white/10 bg-ink-900/90 shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]">
      {/* Title bar */}
      <div className="flex items-center gap-3 border-b border-white/[0.06] bg-white/[0.025] px-4 py-2.5">
        <div className="flex gap-2" aria-hidden="true">
          <span className="size-3 rounded-full bg-[#ff5f57]/90" />
          <span className="size-3 rounded-full bg-[#febc2e]/90" />
          <span className="size-3 rounded-full bg-[#28c840]/90" />
        </div>
        <div className="mx-auto flex items-center gap-2 text-xs text-slate-400">
          <LogoMark className="size-4" />
          project-assets.zip — Arca
          {encrypted && <Lock className="size-3 text-emerald-400" aria-label="Encrypted" />}
        </div>
        <div className="w-[52px]" aria-hidden="true" />
      </div>

      {/* Toolbar */}
      <div className="flex flex-wrap items-center gap-2 border-b border-white/[0.06] px-3 py-2.5">
        <div className="flex items-center gap-0.5">
          {[
            { icon: ArrowLeft, label: 'Back', disabled: false },
            { icon: ArrowRight, label: 'Forward', disabled: true },
            { icon: ArrowUp, label: 'Up one level', disabled: false },
          ].map(({ icon: Icon, label, disabled }) => (
            <button
              key={label}
              type="button"
              aria-label={label}
              disabled={disabled}
              onClick={() => flash(`${label}. Also Alt+← / Alt+→ and the mouse side buttons.`)}
              className="inline-flex size-8 items-center justify-center rounded-lg text-slate-400 transition hover:bg-white/[0.06] hover:text-white disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <Icon className="size-4" />
            </button>
          ))}
        </div>
        <div className="hidden min-w-0 items-center gap-1.5 text-xs text-slate-400 md:flex">
          <FileArchive className="size-3.5 text-brand-300" />
          project-assets.zip
          <ChevronRight className="size-3 text-slate-600" />
          <span className="text-slate-200">design</span>
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <label className="relative">
            <span className="sr-only">Filter entries</span>
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-slate-500" />
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter"
              className="h-8 w-28 rounded-lg border border-white/10 bg-black/30 pl-8 pr-2 text-xs text-slate-200 transition placeholder:text-slate-600 focus:border-brand-400/50 focus:outline-none sm:w-40"
            />
          </label>
          <button
            type="button"
            onClick={() => {
              setEncrypted((v) => !v);
              flash(encrypted ? 'Password removed without recompressing' : 'AES-256 applied without recompressing');
            }}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 text-xs text-slate-300 transition hover:border-white/20 hover:text-white"
          >
            {encrypted ? <Lock className="size-3.5 text-emerald-400" /> : <KeyRound className="size-3.5" />}
            <span className="hidden sm:inline">{encrypted ? 'Remove password' : 'Set password…'}</span>
          </button>
          <button
            type="button"
            onClick={() => run('Extracting 9 entries…', 'Extracted 9 entries to ~/Desktop/project-assets')}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-linear-to-r from-brand-300 to-brand-500 px-3 text-xs font-medium text-ink-950 shadow-[0_6px_20px_-8px_rgba(47,107,255,0.8)] transition hover:brightness-110 active:scale-95"
          >
            <Download className="size-3.5" />
            Extract all
          </button>
        </div>
      </div>

      {/* Header */}
      <div
        aria-hidden="true"
        className={cn('grid items-center gap-3 border-b border-white/[0.06] px-4 py-2 text-[11px] font-medium uppercase tracking-wider text-slate-500', GRID_COLS)}
      >
        <span />
        <span>Name</span>
        <span className="text-right">Size</span>
        <span className="hidden text-right sm:block">Packed</span>
        <span className="hidden sm:block">Method</span>
        <span className="hidden text-right sm:block">Saved</span>
        <span className="hidden lg:block">Modified</span>
      </div>

      {/* Rows */}
      <div
        role="listbox"
        aria-label="Entries in project-assets.zip — arrow keys move, Space ticks, Enter opens"
        aria-multiselectable="true"
        aria-activedescendant={activeId}
        tabIndex={0}
        onKeyDown={onKeyDown}
        className="h-[318px] overflow-y-auto outline-none thin-scrollbar focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-400/50"
      >
        {rows.length === 0 && (
          <div className="flex h-full items-center justify-center text-sm text-slate-500">No entries match “{filter}”.</div>
        )}
        {rows.map((e, i) => {
          const { icon: Icon, cls } = KIND_ICON[e.kind];
          const isTicked = ticked.has(e.name);
          const isCursor = i === safeCursor;
          return (
            <div
              key={e.name}
              id={`arca-row-${i}`}
              role="option"
              aria-selected={isTicked}
              onClick={(ev) => onRowClick(ev, i)}
              onDoubleClick={() => open(e)}
              className={cn(
                'grid cursor-pointer select-none items-center gap-3 px-4 py-2 text-[13px] transition-colors duration-150',
                GRID_COLS,
                isCursor ? 'bg-brand-500/[0.09] shadow-[inset_2px_0_0_#5FAEFF]' : 'hover:bg-white/[0.035]',
              )}
            >
              <span
                role="presentation"
                onClick={(ev) => {
                  ev.stopPropagation();
                  toggle(e.name);
                  setCursor(i);
                  anchor.current = i;
                }}
                className={cn(
                  'flex size-4 items-center justify-center rounded border transition-all duration-200',
                  isTicked ? 'border-brand-400 bg-brand-400 text-ink-950' : 'border-white/20 bg-transparent',
                )}
              >
                {isTicked && <Check className="size-3" strokeWidth={3} />}
              </span>
              <span className="flex min-w-0 items-center gap-2.5">
                <Icon className={cn('size-4 shrink-0', cls)} />
                <span className={cn('truncate', isCursor ? 'text-white' : 'text-slate-200')}>{e.name}</span>
              </span>
              <span className="text-right font-mono text-xs tabular-nums text-slate-400">{e.size}</span>
              <span className="hidden text-right font-mono text-xs tabular-nums text-slate-500 sm:block">{e.packed}</span>
              <span className="hidden font-mono text-xs text-slate-500 sm:block">{e.method}</span>
              <span className={cn('hidden text-right font-mono text-xs tabular-nums sm:block', e.saved !== '0%' && e.saved !== '—' ? 'text-emerald-400/90' : 'text-slate-600')}>
                {e.saved}
              </span>
              <span className="hidden font-mono text-xs text-slate-500 lg:block">{e.modified}</span>
            </div>
          );
        })}
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between gap-3 border-t border-white/[0.06] px-4 py-2 text-[11px] text-slate-500">
        <span>
          {ticked.size} of {ENTRIES.length} ticked
        </span>
        <span className="hidden md:inline">{encrypted ? 'AES-256 · WinZip AE-2' : 'Not encrypted'} · 105.6 MB → 103.2 MB</span>
        <span className="hidden items-center gap-1.5 sm:inline-flex">
          <Kbd className="h-5 min-w-5 text-[10px]">↑</Kbd>
          <Kbd className="h-5 min-w-5 text-[10px]">↓</Kbd> move
          <Kbd className="ml-1 h-5 text-[10px]">Space</Kbd> tick
          <Kbd className="ml-1 h-5 text-[10px]">Ctrl A</Kbd> all
        </span>
      </div>

      {/* Toast */}
      <div
        role="status"
        aria-live="polite"
        className={cn(
          'pointer-events-none absolute bottom-12 left-1/2 w-[min(92%,380px)] -translate-x-1/2 transition-all duration-400',
          toast ? 'translate-y-0 opacity-100' : 'translate-y-3 opacity-0',
        )}
      >
        {toast && (
          <div className="glass-dark rounded-xl px-4 py-3 text-xs text-slate-200 shadow-[0_20px_50px_-12px_rgba(0,0,0,0.9)]">
            <div className="flex items-center gap-2">
              {toast.progress === null ? <Check className="size-3.5 shrink-0 text-emerald-400" /> : <Download className="size-3.5 shrink-0 text-brand-300" />}
              <span className="truncate">{toast.text}</span>
            </div>
            {toast.progress !== null && (
              <div className="mt-2.5 h-1.5 overflow-hidden rounded-full bg-white/[0.07]">
                <div className="progress-stripes h-full rounded-full bg-linear-to-r from-brand-300 to-brand-500" style={{ width: `${toast.progress}%` }} />
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/* ================================================================== */
/*  Windows Explorer                                                   */
/* ================================================================== */
type Tile = { name: string; kind: 'zip' | 'folder' | 'doc' | 'video' | 'pdf' | 'image'; archive?: boolean };

const TILES: Tile[] = [
  { name: 'photos-2026.zip', kind: 'zip', archive: true },
  { name: 'project', kind: 'folder' },
  { name: 'invoices.tar.gz', kind: 'zip', archive: true },
  { name: 'report.pdf', kind: 'pdf' },
  { name: 'demo.mp4', kind: 'video' },
  { name: 'cover.png', kind: 'image' },
];

const TILE_ICON: Record<Tile['kind'], { icon: IconType; cls: string }> = {
  zip: { icon: FileArchive, cls: 'text-brand-300' },
  folder: { icon: Folder, cls: 'text-amber-300 fill-amber-300/25' },
  doc: { icon: FileText, cls: 'text-slate-300' },
  video: { icon: Film, cls: 'text-violet-300' },
  pdf: { icon: FileText, cls: 'text-rose-300' },
  image: { icon: FileImage, cls: 'text-sky-300' },
};

const TOTAL_FILES = 1513;

export function ExplorerMock({ onOpenInArca }: { onOpenInArca: () => void }) {
  const [menuOpen, setMenuOpen] = useState(true);
  const [selected, setSelected] = useState('photos-2026.zip');
  const [progress, setProgress] = useState<number | null>(null);
  const [finished, setFinished] = useState(false);
  const timers = useRef<number[]>([]);

  useEffect(
    () => () => {
      timers.current.forEach((t) => {
        window.clearInterval(t);
        window.clearTimeout(t);
      });
    },
    [],
  );

  const extract = () => {
    setMenuOpen(false);
    setFinished(false);
    timers.current.forEach((t) => window.clearInterval(t));
    const start = performance.now();
    setProgress(0);
    const id = window.setInterval(() => {
      const p = Math.min(100, ((performance.now() - start) / 2800) * 100);
      setProgress(p);
      if (p >= 100) {
        window.clearInterval(id);
        setFinished(true);
        timers.current.push(
          window.setTimeout(() => {
            setProgress(null);
            setFinished(false);
          }, 2600),
        );
      }
    }, 40);
    timers.current.push(id);
  };

  const cancel = () => {
    timers.current.forEach((t) => window.clearInterval(t));
    setProgress(null);
    setFinished(false);
  };

  const menuItems: { label: string; icon: IconType | null; hint?: string; onClick: () => void; brand?: boolean }[] = [
    { label: 'Open with Arca', icon: null, hint: 'Enter', onClick: onOpenInArca, brand: true },
    { label: 'Extract here', icon: FolderOpen, onClick: extract },
    { label: 'Extract to “photos-2026\\”', icon: FolderOpen, onClick: extract },
  ];

  const files = progress === null ? 0 : Math.round((progress / 100) * TOTAL_FILES);

  return (
    <div
      className="relative overflow-hidden rounded-2xl border border-white/10 bg-[#101018] shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]"
      onClick={() => setMenuOpen(false)}
    >
      {/* Title bar */}
      <div className="flex items-center justify-between border-b border-white/[0.06] bg-white/[0.03] pl-4 text-xs text-slate-400">
        <div className="flex items-center gap-2 py-2.5">
          <Folder className="size-4 fill-amber-300/25 text-amber-300" />
          Downloads — File Explorer
        </div>
        <div className="flex" aria-hidden="true">
          <span className="px-4 py-2.5">—</span>
          <span className="px-4 py-2.5">▢</span>
          <span className="px-4 py-2.5">✕</span>
        </div>
      </div>

      {/* Address bar */}
      <div className="flex items-center gap-2 border-b border-white/[0.06] px-3 py-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md border border-white/[0.08] bg-black/30 px-3 py-1.5 text-xs text-slate-400">
          <Monitor className="size-3.5 shrink-0" /> This PC <ChevronRight className="size-3" />
          <span className="truncate text-slate-200">Downloads</span>
        </div>
        <div className="hidden w-44 items-center gap-2 rounded-md border border-white/[0.08] bg-black/30 px-3 py-1.5 text-xs text-slate-600 sm:flex">
          <Search className="size-3.5" /> Search Downloads
        </div>
      </div>

      <div className="flex min-h-[360px]">
        {/* Nav pane */}
        <nav aria-label="Explorer folders (illustration)" className="hidden w-44 shrink-0 border-r border-white/[0.06] p-2 text-xs text-slate-400 md:block">
          {['Home', 'Desktop', 'Downloads', 'Documents', 'Pictures'].map((n) => (
            <div key={n} className={cn('flex items-center gap-2 rounded-md px-2.5 py-1.5', n === 'Downloads' && 'bg-white/[0.07] text-slate-100')}>
              <Folder className="size-3.5 fill-amber-300/20 text-amber-300/80" />
              {n}
            </div>
          ))}
        </nav>

        {/* Files */}
        <div className="relative flex-1 p-4">
          <div className="grid grid-cols-3 gap-2 sm:grid-cols-4 lg:grid-cols-6">
            {TILES.map((t) => {
              const { icon: Icon, cls } = TILE_ICON[t.kind];
              const isSel = selected === t.name;
              return (
                <button
                  key={t.name}
                  type="button"
                  onClick={(ev) => {
                    ev.stopPropagation();
                    setSelected(t.name);
                    setMenuOpen(t.name === 'photos-2026.zip');
                  }}
                  onContextMenu={(ev) => {
                    ev.preventDefault();
                    ev.stopPropagation();
                    setSelected(t.name);
                    setMenuOpen(t.name === 'photos-2026.zip');
                  }}
                  className={cn(
                    'flex flex-col items-center gap-2 rounded-lg border px-2 py-3 text-center transition',
                    isSel ? 'border-sky-400/30 bg-sky-400/[0.1]' : 'border-transparent hover:bg-white/[0.04]',
                  )}
                >
                  <Icon className={cn('size-9', cls)} strokeWidth={1.4} />
                  <span className="line-clamp-2 break-all text-[11px] leading-tight text-slate-300">{t.name}</span>
                </button>
              );
            })}
          </div>

          {!menuOpen && progress === null && (
            <p className="mt-6 text-center text-xs text-slate-500">
              Right-click <span className="font-mono text-slate-300">photos-2026.zip</span> to bring the menu back.
            </p>
          )}

          {/* Context menu */}
          <div
            role="menu"
            aria-label="Context menu for photos-2026.zip"
            onClick={(ev) => ev.stopPropagation()}
            className={cn(
              'glass-dark z-20 mt-4 w-full max-w-[290px] origin-top-left rounded-xl p-1.5 shadow-[0_24px_60px_-12px_rgba(0,0,0,0.9)] transition-all duration-300 sm:absolute sm:left-[22%] sm:top-[42%] sm:mt-0',
              menuOpen ? 'visible scale-100 opacity-100' : 'invisible scale-95 opacity-0',
            )}
          >
            <div className="flex items-center justify-around border-b border-white/[0.06] px-1 pb-1.5 pt-0.5" aria-hidden="true">
              {[Scissors, Copy, Pencil, Share2].map((Icon, i) => (
                <span key={i} className="inline-flex size-8 items-center justify-center rounded-md text-slate-400">
                  <Icon className="size-4" />
                </span>
              ))}
            </div>
            <div className="py-1">
              {menuItems.map((m) => (
                <button
                  key={m.label}
                  type="button"
                  role="menuitem"
                  onClick={m.onClick}
                  className="flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-[13px] text-slate-200 transition hover:bg-white/[0.07]"
                >
                  {m.brand ? <LogoMark className="size-4" /> : m.icon ? <m.icon className="size-4 text-slate-400" /> : null}
                  <span className={cn('flex-1', m.brand && 'font-medium text-white')}>{m.label}</span>
                  {m.hint && <span className="text-[11px] text-slate-500">{m.hint}</span>}
                </button>
              ))}
            </div>
            <div className="border-t border-white/[0.06] pt-1" aria-hidden="true">
              <div className="flex items-center gap-3 px-2.5 py-2 text-[13px] text-slate-500">
                <Settings2 className="size-4" /> <span className="flex-1">Properties</span>
                <span className="text-[11px]">Alt+Enter</span>
              </div>
              <div className="flex items-center gap-3 px-2.5 py-2 text-[13px] text-slate-500">
                <span className="size-4" /> <span className="flex-1">Show more options</span>
              </div>
            </div>
          </div>

          {/* Progress window */}
          <div
            role="status"
            aria-live="polite"
            className={cn(
              'absolute bottom-4 right-4 w-[min(calc(100%-2rem),300px)] transition-all duration-500',
              progress !== null ? 'translate-y-0 opacity-100' : 'pointer-events-none translate-y-4 opacity-0',
            )}
          >
            {progress !== null && (
              <div className="glass-dark rounded-xl p-4 shadow-[0_24px_60px_-12px_rgba(0,0,0,0.9)]">
                <div className="flex items-center gap-2 text-xs text-slate-300">
                  <LogoMark className="size-4" />
                  <span className="flex-1 truncate">{finished ? 'Done' : 'Extracting photos-2026.zip'}</span>
                  {!finished && (
                    <button type="button" aria-label="Cancel extraction" onClick={cancel} className="rounded p-0.5 text-slate-500 transition hover:bg-white/10 hover:text-white">
                      <X className="size-3.5" />
                    </button>
                  )}
                </div>
                <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/[0.07]">
                  <div
                    className={cn('h-full rounded-full', finished ? 'bg-emerald-400' : 'progress-stripes bg-linear-to-r from-brand-300 to-brand-500')}
                    style={{ width: `${progress}%` }}
                  />
                </div>
                <div className="mt-2 flex justify-between font-mono text-[11px] text-slate-500">
                  <span>{finished ? `${TOTAL_FILES.toLocaleString('en-US')} files · 6.28 GB` : `${files.toLocaleString('en-US')} of ${TOTAL_FILES.toLocaleString('en-US')} files`}</span>
                  <span>{finished ? <Check className="inline size-3.5 text-emerald-400" /> : `${Math.round(progress)}%`}</span>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
