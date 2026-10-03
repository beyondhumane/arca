import { useEffect, useRef } from 'react';
import type { MouseEvent as ReactMouseEvent } from 'react';
import { ArrowRight } from 'lucide-react';
import { usePrefersReducedMotion } from '@/lib/hooks';
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
import { LangProvider, storeLang, useCopy } from '@/lib/i18n';
import type { Lang } from '@/lib/i18n';
import { docPath, homePath, RouterProvider, routePath, useRoute, useRouter } from '@/lib/router';
import { pageMeta } from '@/lib/seo';

const COPY = {
  en: {
    skip: 'Skip to content',
    missing: 'Page not found',
    noPage: 'There is nothing at this address. It may have moved when the docs got their own pages.',
    home: 'Home page',
    docs: 'Documentation',
  },
  es: {
    skip: 'Saltar al contenido',
    missing: 'Página no encontrada',
    noPage: 'No hay nada en esta dirección. Puede que se moviera cuando la documentación pasó a tener páginas propias.',
    home: 'Página de inicio',
    docs: 'Documentación',
  },
};

function SkipLink() {
  const t = useCopy(COPY);
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
      {t.skip}
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

function NotFound({ lang }: { lang: Lang }) {
  const t = useCopy(COPY);
  return (
    <main id="main" tabIndex={-1} className="mx-auto max-w-2xl px-5 pb-24 pt-40 text-center outline-none">
      <p className="font-mono text-sm text-brand-300">404</p>
      <h1 className="mt-3 text-4xl font-semibold tracking-[-0.035em] text-white">{t.missing}</h1>
      <p className="mt-4 text-slate-400">{t.noPage}</p>
      <div className="mt-8 flex flex-wrap justify-center gap-3">
        <a
          href={homePath(lang)}
          className="inline-flex items-center gap-1.5 rounded-full bg-white px-5 py-2.5 text-sm font-medium text-ink-950 transition hover:-translate-y-0.5"
        >
          {t.home} <ArrowRight className="size-4" />
        </a>
        <a
          href={docPath(lang)}
          className="inline-flex items-center gap-1.5 rounded-full border border-white/15 px-5 py-2.5 text-sm font-medium text-white transition hover:-translate-y-0.5"
        >
          {t.docs} <ArrowRight className="size-4" />
        </a>
      </div>
    </main>
  );
}

export function Site({ url }: { url: string }) {
  return (
    <RouterProvider url={url}>
      <Localized />
    </RouterProvider>
  );
}

function Localized() {
  const { route, navigate } = useRouter();
  const setLang = (l: Lang) => {
    storeLang(l);
    navigate(routePath(route, l));
  };
  return (
    <LangProvider lang={route.lang} setLang={setLang}>
      <App />
    </LangProvider>
  );
}

function App() {
  const route = useRoute();
  const lang = route.lang;
  const reduced = usePrefersReducedMotion();
  const prevView = useRef(route.view);
  const firstRun = useRef(true);

  useEffect(() => {
    const switched = prevView.current !== route.view;
    const first = firstRun.current;
    prevView.current = route.view;
    firstRun.current = false;

    document.title = pageMeta(route).title;
    if (route.view === 'missing') return;
    if (route.view === 'docs') {
      const section = route.section;
      const raf = requestAnimationFrame(() => {
        const el = section ? document.getElementById(section) : null;
        if (el) el.scrollIntoView({ behavior: 'instant', block: 'start' });
        else window.scrollTo({ top: 0, behavior: 'instant' });
      });
      return () => cancelAnimationFrame(raf);
    }

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
  }, [route, reduced, lang]);

  return (
    <div className="relative min-h-screen overflow-x-clip bg-ink-950 text-slate-300">
      <SkipLink />
      <Navbar view={route.view === 'home' ? 'home' : 'docs'} />
      {route.view === 'home' ? <Landing /> : route.view === 'docs' ? <DocsPage slug={route.slug} /> : <NotFound lang={lang} />}
      <Footer />
      <div aria-hidden="true" className="grain pointer-events-none fixed inset-0 z-[60] opacity-[0.035]" />
    </div>
  );
}
