import { StrictMode } from 'react';
import type { ReactNode } from 'react';
import { renderToStaticMarkup, renderToString } from 'react-dom/server';
import { Site } from './App';
import { DOCS } from './components/docs/registry';
import { FAQS } from './components/landing/FAQ';
import { LANGS } from './lib/i18n';
import type { Lang } from './lib/i18n';
import { docPath, homePath, parseRoute, routePath } from './lib/router';
import type { Route } from './lib/router';
import { pageMeta, SEO } from './lib/seo';
import { LICENSE_URL, RELEASES_URL, REPO_URL, SITE_URL, VERSION } from './lib/site';

export { DOCS, SEO, SITE_URL, REPO_URL, RELEASES_URL, VERSION, LANGS, docPath, homePath };

const OG_IMAGE = `${SITE_URL}/og.png`;
const LICENSE = 'https://www.apache.org/licenses/LICENSE-2.0';

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

const decode = (s: string) =>
  s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#x27;|&#39;/g, "'")
    .replace(/&amp;/g, '&');

const plain = (node: ReactNode) => decode(renderToStaticMarkup(<>{node}</>).replace(/<[^>]+>/g, '')).replace(/\s+/g, ' ').trim();

const abs = (path: string) => `${SITE_URL}${path}`;

function jsonLd(route: Route, url: string, title: string, description: string) {
  const lang = route.lang;
  const org = {
    '@type': 'Organization',
    '@id': `${SITE_URL}/#organization`,
    name: 'Proyecto Arca',
    url: `${SITE_URL}/`,
    logo: `${SITE_URL}/apple-touch-icon.png`,
    sameAs: [REPO_URL],
  };
  const site = {
    '@type': 'WebSite',
    '@id': `${SITE_URL}/#website`,
    url: `${SITE_URL}/`,
    name: 'Arca',
    inLanguage: LANGS,
    publisher: { '@id': org['@id'] },
  };
  if (route.view === 'home') {
    const app = {
      '@type': 'SoftwareApplication',
      '@id': `${SITE_URL}/#software`,
      name: 'Arca',
      description,
      url: abs(homePath(lang)),
      image: OG_IMAGE,
      applicationCategory: 'UtilitiesApplication',
      operatingSystem: 'Windows, macOS, Linux',
      softwareVersion: VERSION,
      license: LICENSE,
      downloadUrl: `${RELEASES_URL}/latest`,
      softwareHelp: { '@type': 'CreativeWork', url: abs(docPath(lang)) },
      offers: { '@type': 'Offer', price: '0', priceCurrency: 'EUR' },
      publisher: { '@id': org['@id'] },
    };
    const code = {
      '@type': 'SoftwareSourceCode',
      '@id': `${SITE_URL}/#source`,
      name: 'Arca',
      codeRepository: REPO_URL,
      programmingLanguage: 'Rust',
      license: LICENSE,
      targetProduct: { '@id': app['@id'] },
    };
    const faq = {
      '@type': 'FAQPage',
      '@id': `${url}#faq`,
      inLanguage: lang,
      mainEntity: FAQS[lang].map((f) => ({
        '@type': 'Question',
        name: f.q,
        acceptedAnswer: { '@type': 'Answer', text: plain(f.a) },
      })),
    };
    return { '@context': 'https://schema.org', '@graph': [org, site, app, code, faq] };
  }
  if (route.view === 'docs') {
    const page = DOCS[lang].find((p) => p.slug === route.slug)!;
    const article = {
      '@type': 'TechArticle',
      '@id': `${url}#article`,
      headline: page.title,
      name: title,
      description,
      url,
      inLanguage: lang,
      ...(page.updated ? { dateModified: page.updated } : {}),
      isPartOf: { '@id': site['@id'] },
      publisher: { '@id': org['@id'] },
      about: { '@type': 'SoftwareApplication', name: 'Arca', url: abs(homePath(lang)) },
      license: LICENSE_URL,
    };
    const crumbs = {
      '@type': 'BreadcrumbList',
      itemListElement: [
        { '@type': 'ListItem', position: 1, name: 'Arca', item: abs(homePath(lang)) },
        { '@type': 'ListItem', position: 2, name: SEO[lang].docs, item: abs(docPath(lang)) },
        { '@type': 'ListItem', position: 3, name: page.title, item: url },
      ],
    };
    return { '@context': 'https://schema.org', '@graph': [org, site, article, crumbs] };
  }
  return null;
}

function head(route: Route): string {
  const { title, description } = pageMeta(route);
  const t = SEO[route.lang];
  const tags = [`<title>${esc(title)}</title>`, `<meta name="description" content="${esc(description)}" />`];
  if (route.view === 'missing') {
    tags.push('<meta name="robots" content="noindex" />');
    return tags.join('\n    ');
  }
  const canonical = abs(routePath(route));
  tags.push(`<link rel="canonical" href="${canonical}" />`);
  for (const l of LANGS) tags.push(`<link rel="alternate" hreflang="${l}" href="${abs(routePath(route, l))}" />`);
  tags.push(`<link rel="alternate" hreflang="x-default" href="${abs(routePath(route, 'en'))}" />`);
  if (route.view === 'docs') {
    tags.push(`<link rel="alternate" type="text/markdown" href="${abs(routePath(route).replace(/\/$/, '.md'))}" />`);
  }
  const ogTitle = route.view === 'home' ? t.tagline : title;
  tags.push(
    `<meta property="og:type" content="${route.view === 'docs' ? 'article' : 'website'}" />`,
    '<meta property="og:site_name" content="Arca" />',
    `<meta property="og:title" content="${esc(ogTitle)}" />`,
    `<meta property="og:description" content="${esc(description)}" />`,
    `<meta property="og:url" content="${canonical}" />`,
    `<meta property="og:image" content="${OG_IMAGE}" />`,
    '<meta property="og:image:width" content="1200" />',
    '<meta property="og:image:height" content="630" />',
    `<meta property="og:image:alt" content="${esc(t.imageAlt)}" />`,
    `<meta property="og:locale" content="${t.locale}" />`,
    ...LANGS.filter((l) => l !== route.lang).map((l) => `<meta property="og:locale:alternate" content="${SEO[l].locale}" />`),
    '<meta name="twitter:card" content="summary_large_image" />',
    `<meta name="twitter:title" content="${esc(ogTitle)}" />`,
    `<meta name="twitter:description" content="${esc(description)}" />`,
    `<meta name="twitter:image" content="${OG_IMAGE}" />`,
    `<meta name="twitter:image:alt" content="${esc(t.imageAlt)}" />`,
  );
  const ld = jsonLd(route, canonical, title, description);
  if (ld) tags.push(`<script type="application/ld+json">${JSON.stringify(ld).replace(/</g, '\\u003c')}</script>`);
  return tags.join('\n    ');
}

export function render(path: string): { html: string; head: string; lang: Lang } {
  const route = parseRoute(path);
  const html = renderToString(
    <StrictMode>
      <Site url={path} />
    </StrictMode>,
  );
  return { html, head: head(route), lang: route.lang };
}

export function paths(): { path: string; indexable: boolean }[] {
  const out: { path: string; indexable: boolean }[] = [];
  for (const lang of LANGS) {
    out.push({ path: homePath(lang), indexable: true });
    out.push({ path: `${lang === 'en' ? '' : `/${lang}`}/docs/`, indexable: false });
    for (const p of DOCS[lang]) out.push({ path: docPath(lang, p.slug), indexable: true });
  }
  return out;
}
