import { useEffect, useRef, useState } from 'react';
import type { ComponentType, KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent, SVGProps } from 'react';
import {
  ArrowDownUp,
  ArrowUp,
  Braces,
  Check,
  ChevronRight,
  Copy,
  Download,
  Eye,
  ExternalLink,
  FileArchive,
  FileCode,
  FileImage,
  FileText,
  Film,
  Filter,
  Folder,
  FolderOpen,
  FolderPlus,
  HardDrive,
  Home,
  Image as ImageIcon,
  Info,
  Monitor,
  Music,
  Package,
  Pencil,
  RotateCw,
  Scissors,
  Search,
  Settings2,
  Share2,
  Trash2,
  Usb,
  X,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { useCopy } from '@/lib/i18n';
import { Kbd, LogoMark } from '../ui';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

const COPY = {
  en: {
    crumbs: ['This computer', 'home', 'ana', 'Showcase'],
    placesGroup: 'Places',
    devicesGroup: 'Devices',
    places: ['Home', 'Desktop', 'Documents', 'Downloads', 'Pictures'],
    devices: ['This computer', 'Backup 64 GB'],
    paneButtons: ['Reload', 'Up', 'Sort', 'Filter', 'Close pane'],
    filterHint: (n: number) => `Filter ${n} items`,
    items: (n: number) => `${n} items`,
    entries: (n: number) => `${n} entries`,
    tickedCount: (n: number) => `${n} ticked`,
    selectFile: 'Select a file to preview',
    preview: 'Preview',
    archive: 'archive',
    menuDisk: [
      'New folder with selection', 'Open', 'Open with', 'Quick preview', 'Cut', 'Copy', 'Duplicate', 'Rename',
      'Move to…', 'Copy to…', 'Compress…', 'Copy path', 'Copy name', 'Properties', 'Move to Trash', 'Permanently delete',
    ],
    menuArchive: ['Open', 'Extract selection', 'Extract here', 'Extract to…', 'Test', 'Copy path', 'Properties'],
    compressing: (n: number) => `Compressing ${n} items into Showcase.zip…`,
    compressed: 'Showcase.zip written · 4.9 MB · AES-256',
    extracting: (n: string) => `Extracting ${n}…`,
    extracted: (n: string) => `${n} extracted beside the archive`,
    tested: (n: string) => `${n}: every CRC checks out`,
    sketch: (l: string) => `${l} works in the window; this is a sketch.`,
    openedNext: (n: string) => `${n} opened in the next pane`,
    openedArchive: (n: string) => `${n} opened in place, like a folder`,
    atTop: 'Already at the top of Showcase',
    rows: 'Folders and files. Arrow keys move, Space ticks, Enter opens, Tab changes pane, right-click for the menu.',
    menuFor: (n: string) => `Context menu for ${n}`,
    move: 'move',
    tick: 'tick',
    open: 'open',
    pane: 'pane',
    menu: ['Open with Arca', 'Extract here', 'Extract to “photos-2026\\”'],
    window: 'Downloads · File Explorer',
    thisPc: 'This PC',
    downloads: 'Downloads',
    search: 'Search Downloads',
    folders: 'Explorer folders (illustration)',
    side: ['Home', 'Desktop', 'Downloads', 'Documents', 'Pictures'],
    rightClick: ['Right-click ', ' to bring the menu back.'],
    menuLabel: 'Context menu for photos-2026.zip',
    properties: 'Properties',
    more: 'Show more options',
    done: 'Done',
    extractingZip: 'Extracting photos-2026.zip',
    cancel: 'Cancel extraction',
    filesDone: (n: string) => `${n} files · 6.28 GB`,
    filesOf: (a: string, b: string) => `${a} of ${b} files`,
    locale: 'en-US',
  },
  es: {
    crumbs: ['Este equipo', 'home', 'ana', 'Showcase'],
    placesGroup: 'Lugares',
    devicesGroup: 'Dispositivos',
    places: ['Inicio', 'Escritorio', 'Documentos', 'Descargas', 'Imágenes'],
    devices: ['Este equipo', 'Backup 64 GB'],
    paneButtons: ['Recargar', 'Subir', 'Ordenar', 'Filtrar', 'Cerrar panel'],
    filterHint: (n: number) => `Filtrar ${n} elementos`,
    items: (n: number) => `${n} elementos`,
    entries: (n: number) => `${n} entradas`,
    tickedCount: (n: number) => `${n} marcados`,
    selectFile: 'Selecciona un archivo para verlo',
    preview: 'Vista previa',
    archive: 'archivo',
    menuDisk: [
      'Nueva carpeta con la selección', 'Abrir', 'Abrir con', 'Vista rápida', 'Cortar', 'Copiar', 'Duplicar', 'Renombrar',
      'Mover a…', 'Copiar a…', 'Comprimir…', 'Copiar ruta', 'Copiar nombre', 'Propiedades', 'Mover a la papelera', 'Eliminar permanentemente',
    ],
    menuArchive: ['Abrir', 'Extraer selección', 'Extraer aquí', 'Extraer en…', 'Comprobar', 'Copiar ruta', 'Propiedades'],
    compressing: (n: number) => `Comprimiendo ${n} elementos en Showcase.zip…`,
    compressed: 'Showcase.zip creado · 4.9 MB · AES-256',
    extracting: (n: string) => `Extrayendo ${n}…`,
    extracted: (n: string) => `${n} extraído junto al archivo`,
    tested: (n: string) => `${n}: todos los CRC correctos`,
    sketch: (l: string) => `${l} funciona en la ventana; esto es un boceto.`,
    openedNext: (n: string) => `${n} abierta en el panel de al lado`,
    openedArchive: (n: string) => `${n} abierto en el sitio, como una carpeta`,
    atTop: 'Ya estás en la raíz de Showcase',
    rows: 'Carpetas y archivos. Las flechas mueven, Space marca, Enter abre, Tab cambia de panel, clic derecho para el menú.',
    menuFor: (n: string) => `Menú contextual de ${n}`,
    move: 'mover',
    tick: 'marcar',
    open: 'abrir',
    pane: 'panel',
    menu: ['Abrir con Arca', 'Extraer aquí', 'Extraer en «photos-2026\\»'],
    window: 'Descargas · Explorador de archivos',
    thisPc: 'Este equipo',
    downloads: 'Descargas',
    search: 'Buscar en Descargas',
    folders: 'Carpetas del Explorador (ilustración)',
    side: ['Inicio', 'Escritorio', 'Descargas', 'Documentos', 'Imágenes'],
    rightClick: ['Haz clic derecho en ', ' para volver a abrir el menú.'],
    menuLabel: 'Menú contextual de photos-2026.zip',
    properties: 'Propiedades',
    more: 'Mostrar más opciones',
    done: 'Hecho',
    extractingZip: 'Extrayendo photos-2026.zip',
    cancel: 'Cancelar la extracción',
    filesDone: (n: string) => `${n} archivos · 6.28 GB`,
    filesOf: (a: string, b: string) => `${a} de ${b} archivos`,
    locale: 'es-ES',
  },
};

/* ================================================================== */
/*  Desktop window                                                     */
/* ================================================================== */
type Kind = 'folder' | 'image' | 'audio' | 'video' | 'doc' | 'code' | 'data' | 'zip';

const KIND_ICON: Record<Kind, { icon: IconType; cls: string }> = {
  folder: { icon: Folder, cls: 'text-brand-300' },
  image: { icon: FileImage, cls: 'text-sky-300' },
  audio: { icon: Music, cls: 'text-violet-300' },
  video: { icon: Film, cls: 'text-violet-300' },
  doc: { icon: FileText, cls: 'text-slate-300' },
  code: { icon: FileCode, cls: 'text-teal-300' },
  data: { icon: Braces, cls: 'text-lime-300' },
  zip: { icon: FileArchive, cls: 'text-amber-300' },
};

type Node = {
  name: string;
  kind: Kind;
  size: string;
  modified: string;
  mime: string;
  children?: Node[];
  lines?: string[];
};

const RUST = [
  '//! Colour for the viewer’s text, one line at a time.',
  '',
  'use eframe::egui::{Color32, FontId};',
  '',
  '#[derive(Clone, Copy, PartialEq, Eq, Debug)]',
  'pub(crate) enum Family {',
  '    Shell,',
  '    Rust,',
  '    Python,',
  '    Markdown,',
  '    Plain,',
  '}',
  '',
  'pub(crate) fn family_of(name: &str) -> Family {',
  '    match name.rsplit_once(\'.\').map(|(_, ext)| ext) {',
  '        Some("rs") => Family::Rust,',
  '        Some("sh") => Family::Shell,',
  '        Some("py") => Family::Python,',
  '        Some("md") => Family::Markdown,',
  '        _ => Family::Plain,',
  '    }',
  '}',
];

const NOTES = ['# Trip notes', '', '- Flights booked', '- Hotel: Lisbon, 3 nights', '- Pack the camera', '- Back up the photos before leaving'];

const PHOTOS: Node[] = [
  { name: 'IMG_0412.jpg', kind: 'image', size: '2.4 MB', modified: '2026-09-14 09:12', mime: 'image/jpeg' },
  { name: 'IMG_0413.jpg', kind: 'image', size: '1.8 MB', modified: '2026-09-14 09:13', mime: 'image/jpeg' },
  { name: 'IMG_0418.jpg', kind: 'image', size: '0.9 MB', modified: '2026-09-14 09:20', mime: 'image/jpeg' },
];

const PROJECTS: Node[] = [
  { name: 'build.sh', kind: 'code', size: '3.4 KB', modified: '2026-10-02 18:40', mime: 'text/x-shellscript', lines: ['#!/usr/bin/env bash', 'set -euo pipefail', '', 'cargo build --release', 'cp target/release/arca-gui dist/'] },
  { name: 'Cargo.toml', kind: 'data', size: '1.1 KB', modified: '2026-10-02 18:40', mime: 'text/x-toml', lines: ['[package]', 'name = "arca-gui"', 'edition = "2024"', '', '[dependencies]', 'eframe = "0.36"'] },
  { name: 'main.rs', kind: 'code', size: '21.2 KB', modified: '2026-10-03 11:05', mime: 'text/x-rust', lines: RUST },
];

const ROOT: Node[] = [
  { name: 'Backups', kind: 'folder', size: '—', modified: '2026-09-30 22:10', mime: 'inode/directory', children: [
    { name: 'vault.7z', kind: 'zip', size: '812 MB', modified: '2026-09-30 22:10', mime: 'application/x-7z-compressed', children: PROJECTS },
    { name: 'notes-2025.tar.gz', kind: 'zip', size: '48 KB', modified: '2026-01-02 09:00', mime: 'application/gzip', children: [] },
  ] },
  { name: 'Music', kind: 'folder', size: '—', modified: '2026-08-21 20:02', mime: 'inode/directory', children: [
    { name: 'track-01.flac', kind: 'audio', size: '5.2 MB', modified: '2026-08-21 20:02', mime: 'audio/flac' },
    { name: 'track-02.flac', kind: 'audio', size: '4.7 MB', modified: '2026-08-21 20:03', mime: 'audio/flac' },
  ] },
  { name: 'Photos', kind: 'folder', size: '—', modified: '2026-09-14 09:20', mime: 'inode/directory', children: PHOTOS },
  { name: 'Projects', kind: 'folder', size: '—', modified: '2026-10-03 11:05', mime: 'inode/directory', children: PROJECTS },
  { name: 'notes.md', kind: 'doc', size: '142 B', modified: '2026-10-01 08:14', mime: 'text/markdown', lines: NOTES },
  { name: 'photos-2026.zip', kind: 'zip', size: '5.1 MB', modified: '2026-10-05 17:52', mime: 'application/zip', children: PHOTOS },
  { name: 'projects-backup.tar.gz', kind: 'zip', size: '8.3 KB', modified: '2026-10-05 17:52', mime: 'application/gzip', children: PROJECTS },
];

const opensInPane = (n: Node) => n.kind === 'folder' || n.kind === 'zip';

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

type Menu = { pane: number; node: Node; x: number; y: number } | null;

const MENU_ICONS: IconType[] = [FolderPlus, ExternalLink, ExternalLink, Eye, Scissors, Copy, Copy, Pencil, FolderOpen, FolderOpen, Package, Share2, Share2, Info, Trash2, Trash2];
const MENU_ARCHIVE_ICONS: IconType[] = [ExternalLink, Download, Download, Download, Check, Share2, Info];
const MENU_BREAKS = new Set([1, 4, 8, 10, 11, 14]);
const MENU_ARCHIVE_BREAKS = new Set([1, 5]);

export function DesktopMock() {
  const tx = useCopy(COPY);
  // The second pane shows what was picked in the first; `stack` is its path
  // inside that folder or archive.
  const [stack, setStack] = useState<Node[]>([ROOT[3]]);
  const [active, setActive] = useState(1);
  const [cursor, setCursor] = useState<[number, number]>([3, 2]);
  const [ticked, setTicked] = useState<Set<string>>(() => new Set(['Projects/main.rs']));
  const [filters, setFilters] = useState<[string | null, string | null]>([null, null]);
  const [menu, setMenu] = useState<Menu>(null);
  const { toast, flash, run } = useToast();
  const frame = useRef<HTMLDivElement>(null);

  const second = stack.length ? stack[stack.length - 1] : null;
  const inArchive = stack.some((n) => n.kind === 'zip');
  const lists: Node[][] = [ROOT, second?.children ?? []];
  const rows = lists.map((list, pane) => {
    const f = (filters[pane] ?? '').trim().toLowerCase();
    return f ? list.filter((n) => n.name.toLowerCase().includes(f)) : list;
  });
  const panes = second ? 2 : 1;
  const current = (pane: number) => rows[pane][Math.min(cursor[pane], rows[pane].length - 1)] ?? null;
  const previewed = current(active);
  const keyOf = (pane: number, n: Node) => (pane === 0 ? n.name : `${stack.map((s) => s.name).join('/')}/${n.name}`);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener('pointerdown', close);
    window.addEventListener('keydown', close);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', close);
    };
  }, [menu]);

  const setCursorAt = (pane: number, idx: number) =>
    setCursor((c) => (pane === 0 ? [idx, c[1]] : [c[0], idx]));

  const toggle = (key: string) =>
    setTicked((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const openNode = (pane: number, n: Node) => {
    if (!opensInPane(n)) {
      flash(tx.sketch(tx.menuDisk[1]));
      return;
    }
    if (pane === 0) {
      setStack([n]);
      setCursor((c) => [c[0], 0]);
      setActive(1);
    } else {
      setStack((s) => [...s, n]);
      setCursor((c) => [c[0], 0]);
    }
    flash(n.kind === 'zip' ? tx.openedArchive(n.name) : tx.openedNext(n.name));
  };

  const up = () => {
    if (stack.length > 1) {
      setStack((s) => s.slice(0, -1));
      setCursor((c) => [c[0], 0]);
    } else if (stack.length === 1) {
      setStack([]);
      setActive(0);
    } else {
      flash(tx.atTop);
    }
  };

  const onKeyDown = (ev: ReactKeyboardEvent<HTMLDivElement>) => {
    const list = rows[active];
    const last = list.length - 1;
    const idx = Math.min(cursor[active], last);
    switch (ev.key) {
      case 'ArrowDown':
        ev.preventDefault();
        setCursorAt(active, Math.min(last, idx + 1));
        break;
      case 'ArrowUp':
        ev.preventDefault();
        setCursorAt(active, Math.max(0, idx - 1));
        break;
      case 'ArrowRight':
        ev.preventDefault();
        if (panes > 1) setActive(1);
        break;
      case 'ArrowLeft':
        ev.preventDefault();
        setActive(0);
        break;
      case 'Tab':
        if (panes > 1) {
          ev.preventDefault();
          setActive((a) => (a + 1) % panes);
        }
        break;
      case ' ':
        ev.preventDefault();
        if (list[idx]) toggle(keyOf(active, list[idx]));
        break;
      case 'Enter':
        ev.preventDefault();
        if (list[idx]) openNode(active, list[idx]);
        break;
      case 'Backspace':
        ev.preventDefault();
        if (active === 1) up();
        else flash(tx.atTop);
        break;
      case 'Escape':
        setMenu(null);
        break;
      default:
        if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'a') {
          ev.preventDefault();
          const keys = list.map((n) => keyOf(active, n));
          setTicked((prev) => {
            const all = keys.every((k) => prev.has(k));
            const next = new Set(prev);
            keys.forEach((k) => (all ? next.delete(k) : next.add(k)));
            return next;
          });
        }
    }
  };

  const onRowClick = (ev: ReactMouseEvent<HTMLDivElement>, pane: number, idx: number) => {
    const n = rows[pane][idx];
    setActive(pane);
    setCursorAt(pane, idx);
    if (ev.ctrlKey || ev.metaKey) {
      toggle(keyOf(pane, n));
      return;
    }
    if (pane === 0 && opensInPane(n)) openNode(0, n);
  };

  const onContextMenu = (ev: ReactMouseEvent<HTMLDivElement>, pane: number, idx: number) => {
    ev.preventDefault();
    const box = frame.current?.getBoundingClientRect();
    if (!box) return;
    setActive(pane);
    setCursorAt(pane, idx);
    setMenu({ pane, node: rows[pane][idx], x: ev.clientX - box.left, y: ev.clientY - box.top });
  };

  const tickedIn = (pane: number) => rows[pane].filter((n) => ticked.has(keyOf(pane, n))).length;

  const act = (label: string, index: number) => {
    const n = menu?.node;
    setMenu(null);
    if (!n) return;
    if (!inArchive || menu?.pane === 0) {
      if (index === 1) openNode(menu?.pane ?? 0, n);
      else if (index === 10) run(tx.compressing(Math.max(1, tickedIn(menu?.pane ?? 0))), tx.compressed);
      else flash(tx.sketch(label));
    } else if (index === 0) openNode(1, n);
    else if (index >= 1 && index <= 3) run(tx.extracting(n.name), tx.extracted(n.name));
    else if (index === 4) run(tx.tested(n.name), tx.tested(n.name), 900);
    else flash(tx.sketch(label));
  };

  const menuItems = menu && (inArchive && menu.pane === 1 ? tx.menuArchive : tx.menuDisk);
  const menuIcons = menu && (inArchive && menu.pane === 1 ? MENU_ARCHIVE_ICONS : MENU_ICONS);
  const menuBreaks = menu && (inArchive && menu.pane === 1 ? MENU_ARCHIVE_BREAKS : MENU_BREAKS);

  const paneHeader = (pane: number, title: string) => (
    <div className="flex h-9 items-center gap-0.5 border-b border-white/[0.06] px-2">
      <span className={cn('mr-auto truncate pl-1 text-xs', active === pane ? 'text-white' : 'text-slate-400')}>{title}</span>
      {[
        { icon: RotateCw, label: tx.paneButtons[0], onClick: () => flash(tx.sketch(tx.paneButtons[0])) },
        { icon: ArrowUp, label: tx.paneButtons[1], onClick: () => (pane === 1 ? up() : flash(tx.atTop)) },
        { icon: ArrowDownUp, label: tx.paneButtons[2], onClick: () => flash(tx.sketch(tx.paneButtons[2])) },
        {
          icon: Filter,
          label: tx.paneButtons[3],
          onClick: () =>
            setFilters((f) => {
              const next: [string | null, string | null] = [...f];
              next[pane] = next[pane] === null ? '' : null;
              return next;
            }),
          on: filters[pane] !== null,
        },
        ...(pane === 1 ? [{ icon: X, label: tx.paneButtons[4], onClick: () => { setStack([]); setActive(0); } }] : []),
      ].map(({ icon: Icon, label, onClick, on }) => (
        <button
          key={label}
          type="button"
          aria-label={label}
          aria-pressed={on}
          onClick={(ev) => {
            ev.stopPropagation();
            onClick();
          }}
          className={cn(
            'inline-flex size-7 items-center justify-center rounded-md transition hover:bg-white/[0.06] hover:text-white',
            on ? 'bg-brand-500/15 text-brand-300' : 'text-slate-500',
          )}
        >
          <Icon className="size-3.5" />
        </button>
      ))}
    </div>
  );

  const paneBody = (pane: number) => {
    const list = rows[pane];
    const idx = Math.min(cursor[pane], list.length - 1);
    return (
      <div className="flex-1 overflow-y-auto py-1 thin-scrollbar">
        {filters[pane] !== null && (
          <label className="mx-2 mb-1 flex items-center gap-2 rounded-md border border-white/10 bg-black/30 px-2">
            <Filter className="size-3 text-slate-500" />
            <input
              autoFocus
              value={filters[pane] ?? ''}
              onChange={(e) => setFilters((f) => (pane === 0 ? [e.target.value, f[1]] : [f[0], e.target.value]))}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder={tx.filterHint(lists[pane].length)}
              aria-label={tx.filterHint(lists[pane].length)}
              className="h-7 w-full bg-transparent text-xs text-slate-200 outline-none placeholder:text-slate-600"
            />
          </label>
        )}
        {list.map((n, i) => {
          const { icon: Icon, cls } = KIND_ICON[n.kind];
          const isTicked = ticked.has(keyOf(pane, n));
          const isCursor = i === idx;
          const isOpen = pane === 0 && stack[0]?.name === n.name;
          return (
            <div
              key={n.name}
              id={`arca-${pane}-${i}`}
              role="option"
              aria-selected={isTicked}
              onClick={(ev) => onRowClick(ev, pane, i)}
              onDoubleClick={() => openNode(pane, n)}
              onContextMenu={(ev) => onContextMenu(ev, pane, i)}
              className={cn(
                'mx-1 flex h-7 cursor-pointer select-none items-center gap-2 rounded-md px-2 text-[13px] transition-colors duration-150',
                isCursor && active === pane
                  ? 'bg-brand-500/[0.14] text-white ring-1 ring-inset ring-brand-400/60'
                  : isOpen || isTicked
                    ? 'bg-white/[0.05] text-slate-100'
                    : 'text-slate-300 hover:bg-white/[0.035]',
              )}
            >
              <span
                role="presentation"
                onClick={(ev) => {
                  ev.stopPropagation();
                  toggle(keyOf(pane, n));
                  setActive(pane);
                  setCursorAt(pane, i);
                }}
                className={cn(
                  'flex size-3.5 shrink-0 items-center justify-center rounded-sm border transition-all duration-200',
                  isTicked ? 'border-brand-400 bg-brand-400 text-ink-950' : 'border-white/15',
                )}
              >
                {isTicked && <Check className="size-2.5" strokeWidth={3} />}
              </span>
              <Icon className={cn('size-4 shrink-0', cls)} />
              <span className="min-w-0 flex-1 truncate">{n.name}</span>
              {n.kind !== 'folder' && <span className="hidden font-mono text-[11px] tabular-nums text-slate-500 sm:inline">{n.size}</span>}
              {opensInPane(n) && <ChevronRight className="size-3.5 shrink-0 text-slate-600" />}
            </div>
          );
        })}
      </div>
    );
  };

  const PreviewIcon = previewed ? KIND_ICON[previewed.kind].icon : Eye;
  const previewCls = previewed ? KIND_ICON[previewed.kind].cls : 'text-slate-500';
  const title = second ? [...tx.crumbs, ...stack.map((n) => n.name)] : tx.crumbs;
  const visible = rows[active].length;

  return (
    <div
      ref={frame}
      className="relative overflow-hidden rounded-2xl border border-white/10 bg-ink-900/90 shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]"
    >
      {/* Title bar with breadcrumbs */}
      <div className="flex h-10 items-center gap-2 border-b border-white/[0.06] bg-white/[0.025] px-3 text-xs text-slate-400">
        <LogoMark className="size-4 shrink-0" />
        <div className="flex min-w-0 items-center gap-1 overflow-hidden">
          {title.map((crumb, i) => (
            <span key={`${crumb}-${i}`} className="flex items-center gap-1">
              {i > 0 && <ChevronRight className="size-3 shrink-0 text-slate-600" />}
              <span className={cn('truncate', i === title.length - 1 ? 'text-white' : '')}>{crumb}</span>
            </span>
          ))}
          {inArchive && <span className="ml-1 rounded bg-amber-300/15 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-amber-200">{tx.archive}</span>}
        </div>
        <div className="ml-auto flex items-center gap-0.5 text-slate-500" aria-hidden="true">
          <span className="inline-flex size-7 items-center justify-center"><Search className="size-3.5" /></span>
          <span className="inline-flex size-7 items-center justify-center"><Settings2 className="size-3.5" /></span>
        </div>
      </div>

      <div className="flex h-[400px]">
        {/* Sidebar */}
        <aside className="hidden w-44 shrink-0 flex-col gap-4 border-r border-white/[0.06] bg-white/[0.015] p-3 text-[12px] md:flex" aria-hidden="true">
          <div>
            <div className="mb-1.5 px-2 text-[10px] font-medium uppercase tracking-wider text-slate-500">{tx.placesGroup}</div>
            {[Home, Monitor, FileText, Download, ImageIcon].map((Icon, i) => (
              <div key={tx.places[i]} className={cn('flex h-7 items-center gap-2 rounded-md px-2', i === 0 ? 'bg-brand-500/[0.12] text-white' : 'text-slate-400')}>
                <Icon className="size-3.5" />
                {tx.places[i]}
              </div>
            ))}
          </div>
          <div>
            <div className="mb-1.5 px-2 text-[10px] font-medium uppercase tracking-wider text-slate-500">{tx.devicesGroup}</div>
            {[HardDrive, Usb].map((Icon, i) => (
              <div key={tx.devices[i]} className="flex h-7 items-center gap-2 rounded-md px-2 text-slate-400">
                <Icon className="size-3.5" />
                {tx.devices[i]}
              </div>
            ))}
          </div>
        </aside>

        {/* Panes */}
        <div
          role="listbox"
          aria-label={tx.rows}
          aria-multiselectable="true"
          aria-activedescendant={current(active) ? `arca-${active}-${Math.min(cursor[active], rows[active].length - 1)}` : undefined}
          tabIndex={0}
          onKeyDown={onKeyDown}
          className="flex min-w-0 flex-1 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-400/50"
        >
          <div
            className={cn('flex min-w-0 flex-col border-r border-white/[0.06]', panes > 1 ? 'w-1/2' : 'flex-1')}
            onPointerEnter={() => setActive(0)}
          >
            {paneHeader(0, tx.crumbs[3])}
            {paneBody(0)}
          </div>
          {second && (
            <div className="flex min-w-0 w-1/2 flex-col" onPointerEnter={() => setActive(1)}>
              {paneHeader(1, second.name)}
              {paneBody(1)}
            </div>
          )}
        </div>

        {/* Preview */}
        <aside className="hidden w-[300px] shrink-0 flex-col border-l border-white/[0.06] lg:flex" aria-label={tx.preview}>
          <div className="flex items-start gap-2.5 border-b border-white/[0.06] px-3 py-2.5">
            <PreviewIcon className={cn('mt-0.5 size-5 shrink-0', previewCls)} />
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px] text-white">{previewed ? previewed.name : tx.preview}</div>
              <div className="truncate text-[11px] text-slate-500">
                {previewed
                  ? previewed.kind === 'folder' || previewed.kind === 'zip'
                    ? `${previewed.kind === 'zip' ? tx.entries(previewed.children?.length ?? 0) : tx.items(previewed.children?.length ?? 0)} · ${previewed.modified} · ${previewed.mime}`
                    : `${previewed.size} · ${previewed.modified} · ${previewed.mime}`
                  : tx.selectFile}
              </div>
            </div>
            <span className="inline-flex size-6 items-center justify-center text-slate-500" aria-hidden="true"><X className="size-3.5" /></span>
          </div>
          <div className="flex-1 overflow-hidden p-3 font-mono text-[11px] leading-5 text-slate-300">
            {previewed?.lines ? (
              previewed.lines.map((line, i) => (
                <div key={i} className="flex gap-3 whitespace-pre">
                  <span className="w-5 shrink-0 select-none text-right text-slate-600">{i + 1}</span>
                  <span className={cn('truncate', line.startsWith('//') || line.startsWith('#!') ? 'italic text-slate-500' : line.startsWith('#') || line.startsWith('[') ? 'text-brand-300' : line.startsWith('- ') ? 'text-slate-200' : '')}>
                    {line}
                  </span>
                </div>
              ))
            ) : previewed?.kind === 'image' ? (
              <div className="flex h-full items-center justify-center rounded-lg bg-linear-to-br from-sky-400/30 via-brand-500/20 to-amber-300/30 text-slate-400">
                <ImageIcon className="size-8" />
              </div>
            ) : previewed ? (
              <div className="pt-2 text-slate-500">
                {previewed.children?.map((c) => (
                  <div key={c.name} className="flex items-center gap-2 py-0.5">
                    <ChevronRight className="size-3" />
                    {c.name}
                  </div>
                ))}
                {previewed.kind === 'audio' && <span>{previewed.mime}</span>}
              </div>
            ) : null}
          </div>
        </aside>
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between gap-3 border-t border-white/[0.06] px-4 py-2 text-[11px] text-slate-500">
        <span>
          {tx.items(visible)} · {tx.tickedCount(ticked.size)}
        </span>
        <span className="hidden items-center gap-1.5 sm:inline-flex">
          <Kbd className="h-5 min-w-5 text-[10px]">↑</Kbd>
          <Kbd className="h-5 min-w-5 text-[10px]">↓</Kbd> {tx.move}
          <Kbd className="ml-1 h-5 text-[10px]">Space</Kbd> {tx.tick}
          <Kbd className="ml-1 h-5 text-[10px]">Enter</Kbd> {tx.open}
          <Kbd className="ml-1 h-5 text-[10px]">Tab</Kbd> {tx.pane}
        </span>
      </div>

      {/* Context menu */}
      {menu && menuItems && menuIcons && menuBreaks && (
        <div
          role="menu"
          aria-label={tx.menuFor(menu.node.name)}
          onPointerDown={(ev) => ev.stopPropagation()}
          style={{ left: Math.min(menu.x, (frame.current?.clientWidth ?? 600) - 236), top: Math.min(menu.y, (frame.current?.clientHeight ?? 480) - 24 * menuItems.length - 60) }}
          className="glass-dark absolute z-10 w-[228px] rounded-xl p-1 text-[12px] text-slate-200 shadow-[0_20px_50px_-12px_rgba(0,0,0,0.9)]"
        >
          {menuItems.map((label, i) => {
            const Icon = menuIcons[i];
            const danger = !inArchive && i >= 14;
            return (
              <div key={label}>
                {menuBreaks.has(i) && i > 0 && <div className="my-1 h-px bg-white/[0.08]" />}
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => act(label, i)}
                  className={cn(
                    'flex h-6 w-full items-center gap-2 rounded-md px-2 text-left transition hover:bg-white/[0.07]',
                    danger ? 'text-rose-300' : '',
                  )}
                >
                  <Icon className="size-3.5 shrink-0 opacity-80" />
                  <span className="flex-1 truncate">{label}</span>
                  {label === tx.menuDisk[2] && !inArchive && <ChevronRight className="size-3 text-slate-500" />}
                </button>
              </div>
            );
          })}
        </div>
      )}

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
              {toast.progress === null ? <Check className="size-3.5 shrink-0 text-emerald-400" /> : <Package className="size-3.5 shrink-0 text-brand-300" />}
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
  const tx = useCopy(COPY);

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
    { label: tx.menu[0], icon: null, hint: 'Enter', onClick: onOpenInArca, brand: true },
    { label: tx.menu[1], icon: FolderOpen, onClick: extract },
    { label: tx.menu[2], icon: FolderOpen, onClick: extract },
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
          {tx.window}
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
          <Monitor className="size-3.5 shrink-0" /> {tx.thisPc} <ChevronRight className="size-3" />
          <span className="truncate text-slate-200">{tx.downloads}</span>
        </div>
        <div className="hidden w-44 items-center gap-2 rounded-md border border-white/[0.08] bg-black/30 px-3 py-1.5 text-xs text-slate-600 sm:flex">
          <Search className="size-3.5" /> {tx.search}
        </div>
      </div>

      <div className="flex min-h-[360px]">
        {/* Nav pane */}
        <nav aria-label={tx.folders} className="hidden w-44 shrink-0 border-r border-white/[0.06] p-2 text-xs text-slate-400 md:block">
          {tx.side.map((n, i) => (
            <div key={n} className={cn('flex items-center gap-2 rounded-md px-2.5 py-1.5', i === 2 && 'bg-white/[0.07] text-slate-100')}>
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
              {tx.rightClick[0]}
              <span className="font-mono text-slate-300">photos-2026.zip</span>
              {tx.rightClick[1]}
            </p>
          )}

          {/* Context menu */}
          <div
            role="menu"
            aria-label={tx.menuLabel}
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
                <Settings2 className="size-4" /> <span className="flex-1">{tx.properties}</span>
                <span className="text-[11px]">Alt+Enter</span>
              </div>
              <div className="flex items-center gap-3 px-2.5 py-2 text-[13px] text-slate-500">
                <span className="size-4" /> <span className="flex-1">{tx.more}</span>
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
                  <span className="flex-1 truncate">{finished ? tx.done : tx.extractingZip}</span>
                  {!finished && (
                    <button type="button" aria-label={tx.cancel} onClick={cancel} className="rounded p-0.5 text-slate-500 transition hover:bg-white/10 hover:text-white">
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
                  <span>{finished ? tx.filesDone(TOTAL_FILES.toLocaleString(tx.locale)) : tx.filesOf(files.toLocaleString(tx.locale), TOTAL_FILES.toLocaleString(tx.locale))}</span>
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
