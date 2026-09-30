import { useEffect, useRef } from 'react';
import type { MouseEvent as ReactMouseEvent } from 'react';
import { useHashRoute, usePrefersReducedMotion } from '@/lib/hooks';
import { Navbar } from '@/components/Navbar';
import { Footer } from '@/components/Footer';
import { GradientDivider } from '@/components/ui';
import { Hero } from '@/components/landing/Hero';
import { SocialProof } from '@/components/landing/SocialProof';
import { Features } from '@/components/landing/Features';
import { Showcase } from '@/components/landing/Showcase';
import { Performance } from '@/components/landing/Performance';
import { Benefits } from '@/components/landing/Benefits';
import { FAQ } from '@/components/landing/FAQ';
import { CTA } from '@/components/landing/CTA';
import { DocsPage } from '@/components/docs/DocsPage';
import { DOC_PAGES } from '@/components/docs/registry';

const HOME_TITLE = 'Arca — The fast, safe, open-source archiver built in Rust';

function SkipLink() {
  const onClick = (e: ReactMouseEvent<HTMLAnchorElement>) => {
    e.preventDefault();
    const main = document.getElementById('main');
    if (main) {
      main.focus({ preventScroll: true });
      main.scrollIntoView({ block: 'start' });
    }
  };
  return (
    <a
      href="#main"
      onClick={onClick}
      className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-[100] focus:rounded-lg focus:bg-white focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-ink-950"
    >
      Skip to content
    </a>
  );
}

function Landing() {
  return (
    <main id="main" tabIndex={-1} className="outline-none">
      <Hero />
      <SocialProof />
      <GradientDivider className="mx-auto max-w-5xl" />
      <Features />
      <Showcase />
      <Performance />
      <Benefits />
      <FAQ />
      <CTA />
    </main>
  );
}

export default function App() {
  const route = useHashRoute();
  const reduced = usePrefersReducedMotion();
  const prevView = useRef(route.view);
  const firstRun = useRef(true);

  useEffect(() => {
    const switched = prevView.current !== route.view;
    const first = firstRun.current;
    prevView.current = route.view;
    firstRun.current = false;

    if (route.view === 'docs') {
      const page = DOC_PAGES.find((p) => p.slug === route.slug);
      document.title = page ? `${page.title} — Arca Docs` : 'Page not found — Arca Docs';
      const section = route.section;
      const raf = requestAnimationFrame(() => {
        const el = section ? document.getElementById(section) : null;
        if (el) el.scrollIntoView({ behavior: 'instant', block: 'start' });
        else window.scrollTo({ top: 0, behavior: 'instant' });
      });
      return () => cancelAnimationFrame(raf);
    }

    document.title = HOME_TITLE;
    const id = route.anchor;
    if (!id) {
      if (switched) window.scrollTo({ top: 0, behavior: 'instant' });
      return;
    }
    const raf = requestAnimationFrame(() => {
      const el = document.getElementById(id);
      if (el) el.scrollIntoView({ behavior: first || switched || reduced ? 'instant' : 'smooth', block: 'start' });
    });
    return () => cancelAnimationFrame(raf);
  }, [route, reduced]);

  return (
    <div className="relative min-h-screen overflow-x-clip bg-ink-950 text-slate-300">
      <SkipLink />
      <Navbar view={route.view} />
      {route.view === 'home' ? <Landing /> : <DocsPage slug={route.slug} />}
      <Footer />
      <div aria-hidden="true" className="grain pointer-events-none fixed inset-0 z-[60] opacity-[0.035]" />
    </div>
  );
}
