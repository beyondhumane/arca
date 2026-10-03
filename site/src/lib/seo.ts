import { DOCS } from '@/components/docs/registry';
import type { DocPage } from '@/components/docs/registry';
import type { Route } from '@/lib/router';

export const SEO = {
  en: {
    home: 'Arca · Open-source archiver in safe Rust',
    description:
      'Arca is an open-source, cross-platform archiver in safe Rust: ZIP and TAR, multi-threaded Zstandard, AES-256 encryption and a native desktop window.',
    tagline: 'Arca · Pack faster. Unpack safer.',
    docs: 'Arca Docs',
    missing: 'Page not found',
    missingDescription: 'This page does not exist. Go back to the Arca home page or the documentation.',
    imageAlt: 'Arca, open-source archiver in safe Rust. Pack faster. Unpack safer.',
    locale: 'en_US',
  },
  es: {
    home: 'Arca · Archivador de código abierto en Rust seguro',
    description:
      'Arca es un archivador de código abierto y multiplataforma en Rust seguro: ZIP y TAR, Zstandard multihilo, cifrado AES-256 y una ventana nativa.',
    tagline: 'Arca · Comprime rápido. Extrae seguro.',
    docs: 'Documentación de Arca',
    missing: 'Página no encontrada',
    missingDescription: 'Esta página no existe. Vuelve a la página de inicio de Arca o a la documentación.',
    imageAlt: 'Arca, archivador de código abierto en Rust seguro. Comprime rápido. Extrae seguro.',
    locale: 'es_ES',
  },
};

export function pageMeta(route: Route): { title: string; description: string; page?: DocPage } {
  const t = SEO[route.lang];
  if (route.view === 'home') return { title: t.home, description: t.description };
  if (route.view === 'docs') {
    const page = DOCS[route.lang].find((p) => p.slug === route.slug);
    if (page) return { title: `${page.title} · ${t.docs}`, description: page.description, page };
  }
  return { title: `${t.missing} · Arca`, description: t.missingDescription };
}
