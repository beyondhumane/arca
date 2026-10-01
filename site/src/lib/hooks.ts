import { useEffect, useMemo, useRef, useState } from 'react';

/* ------------------------------------------------------------------ */
/*  Reduced motion                                                     */
/* ------------------------------------------------------------------ */
const REDUCED_QUERY = '(prefers-reduced-motion: reduce)';

export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState<boolean>(() =>
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia(REDUCED_QUERY).matches
      : false,
  );
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia(REDUCED_QUERY);
    const onChange = () => setReduced(mq.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);
  return reduced;
}

/* ------------------------------------------------------------------ */
/*  In view                                                            */
/* ------------------------------------------------------------------ */
type InViewOptions = { once?: boolean; rootMargin?: string; threshold?: number };

export function useInView<T extends Element = HTMLDivElement>({
  once = true,
  rootMargin = '0px 0px -8% 0px',
  threshold = 0,
}: InViewOptions = {}) {
  const ref = useRef<T>(null);
  const [inView, setInView] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (typeof IntersectionObserver === 'undefined') {
      setInView(true);
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setInView(true);
            if (once) io.disconnect();
          } else if (!once) {
            setInView(false);
          }
        }
      },
      { rootMargin, threshold },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [once, rootMargin, threshold]);

  return [ref, inView] as const;
}

/* ------------------------------------------------------------------ */
/*  Count up                                                           */
/* ------------------------------------------------------------------ */
export function useCountUp(target: number, start: boolean, { duration = 1600, decimals = 0 } = {}) {
  const reduced = usePrefersReducedMotion();
  const [value, setValue] = useState(0);

  useEffect(() => {
    if (!start) return;
    if (reduced) {
      setValue(target);
      return;
    }
    let raf = 0;
    const t0 = performance.now();
    const tick = (now: number) => {
      const p = Math.min(1, (now - t0) / duration);
      const eased = 1 - Math.pow(1 - p, 4);
      setValue(target * eased);
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [start, target, duration, reduced]);

  return value.toFixed(decimals);
}

/* ------------------------------------------------------------------ */
/*  Scroll position                                                    */
/* ------------------------------------------------------------------ */
export function useScrolled(offset = 12): boolean {
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > offset);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, [offset]);
  return scrolled;
}

/* ------------------------------------------------------------------ */
/*  Scroll spy                                                         */
/* ------------------------------------------------------------------ */
export function useScrollSpy(ids: readonly string[], enabled = true): string | null {
  const [active, setActive] = useState<string | null>(null);
  const key = ids.join('|');

  useEffect(() => {
    if (!enabled) {
      setActive(null);
      return;
    }
    const els = key
      .split('|')
      .map((id) => document.getElementById(id))
      .filter((el): el is HTMLElement => el !== null);
    if (els.length === 0 || typeof IntersectionObserver === 'undefined') return;

    const visible = new Map<string, boolean>();
    const io = new IntersectionObserver(
      (entries) => {
        entries.forEach((e) => visible.set(e.target.id, e.isIntersecting));
        const first = els.find((el) => visible.get(el.id));
        setActive(first ? first.id : null);
      },
      { rootMargin: '-38% 0px -56% 0px' },
    );
    els.forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, [key, enabled]);

  return active;
}

/* ------------------------------------------------------------------ */
/*  Hash routing                                                       */
/* ------------------------------------------------------------------ */
export type Route = { view: 'home'; anchor: string } | { view: 'docs'; slug: string; section?: string };

export function parseHash(hash: string): Route {
  const h = hash.replace(/^#/, '');
  if (h === '/docs' || h.startsWith('/docs/')) {
    const [slug, section] = h.slice('/docs'.length).replace(/^\/+/, '').replace(/\/+$/, '').split('/');
    return { view: 'docs', slug: slug || 'introduction', section };
  }
  return { view: 'home', anchor: h.replace(/^\/+/, '') };
}

export function useHashRoute(): Route {
  const [hash, setHash] = useState(() => (typeof window === 'undefined' ? '' : window.location.hash));
  useEffect(() => {
    const onChange = () => setHash(window.location.hash);
    window.addEventListener('hashchange', onChange);
    return () => window.removeEventListener('hashchange', onChange);
  }, []);
  return useMemo(() => parseHash(hash), [hash]);
}

/* ------------------------------------------------------------------ */
/*  GitHub stars (deduplicated, fails silently)                        */
/* ------------------------------------------------------------------ */
let starsPromise: Promise<number | null> | null = null;

function fetchStars(): Promise<number | null> {
  if (!starsPromise) {
    starsPromise = fetch('https://api.github.com/repos/beyondhumane/arca', {
      headers: { Accept: 'application/vnd.github+json' },
    })
      .then((r) => (r.ok ? (r.json() as Promise<unknown>) : null))
      .then((d) => {
        if (d && typeof d === 'object' && 'stargazers_count' in d) {
          const n = (d as { stargazers_count: unknown }).stargazers_count;
          return typeof n === 'number' ? n : null;
        }
        return null;
      })
      .catch(() => null)
      .then((n) => {
        if (n === null) starsPromise = null;
        return n;
      });
  }
  return starsPromise;
}

export function useGitHubStars(): number | null {
  const [stars, setStars] = useState<number | null>(null);
  useEffect(() => {
    let alive = true;
    fetchStars().then((n) => {
      if (alive) setStars(n);
    });
    return () => {
      alive = false;
    };
  }, []);
  return stars;
}

export function formatCount(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`;
  return String(n);
}

/* ------------------------------------------------------------------ */
/*  Operating system detection                                         */
/* ------------------------------------------------------------------ */
export type OS = 'windows' | 'macos' | 'linux' | 'mobile' | 'unknown';

export function detectOS(): OS {
  if (typeof navigator === 'undefined') return 'unknown';
  const nav = navigator as Navigator & { userAgentData?: { platform?: string; mobile?: boolean } };
  const ua = nav.userAgent.toLowerCase();
  const platform = (nav.userAgentData?.platform || nav.platform || '').toLowerCase();
  if (nav.userAgentData?.mobile || /android|iphone|ipad|ipod/.test(ua)) return 'mobile';
  if ((platform.includes('mac') || ua.includes('macintosh')) && nav.maxTouchPoints > 1) return 'mobile';
  if (platform.includes('win') || ua.includes('windows')) return 'windows';
  if (platform.includes('mac') || ua.includes('macintosh')) return 'macos';
  if (platform.includes('linux') || ua.includes('linux') || ua.includes('x11')) return 'linux';
  return 'unknown';
}

export function useOS(): OS {
  const [os] = useState<OS>(detectOS);
  return os;
}

export type MacArch = 'arm' | 'x86';

type HighEntropyUAData = { getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string }> };

export function useMacArch(enabled: boolean): MacArch | null {
  const [arch, setArch] = useState<MacArch | null>(null);
  useEffect(() => {
    const data = (navigator as Navigator & { userAgentData?: HighEntropyUAData }).userAgentData;
    if (!enabled || !data?.getHighEntropyValues) return;
    let alive = true;
    data
      .getHighEntropyValues(['architecture'])
      .then(({ architecture }) => {
        if (alive && (architecture === 'arm' || architecture === 'x86')) setArch(architecture);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [enabled]);
  return arch;
}

/* ------------------------------------------------------------------ */
/*  Text scramble (used for the "decrypt" micro-interaction)           */
/* ------------------------------------------------------------------ */
const GLYPHS = '0123456789abcdef#%&*+=?@';

function scramble(target: string, revealed: number): string {
  let out = '';
  for (let i = 0; i < target.length; i++) {
    const ch = target[i];
    out += i < revealed || ch === ' ' ? ch : GLYPHS[Math.floor(Math.random() * GLYPHS.length)];
  }
  return out;
}

export function useScramble(target: string, active: boolean, interval = 28): string {
  const [text, setText] = useState(() => scramble(target, 0));
  const progress = useRef(0);

  useEffect(() => {
    const len = target.length;
    const id = window.setInterval(() => {
      progress.current = active ? Math.min(len, progress.current + 1) : Math.max(0, progress.current - 1);
      setText(scramble(target, progress.current));
      if ((active && progress.current >= len) || (!active && progress.current <= 0)) {
        window.clearInterval(id);
      }
    }, interval);
    return () => window.clearInterval(id);
  }, [active, target, interval]);

  return text;
}
