import type { Lang } from '@/lib/i18n';

export type DocPage = {
  slug: string;
  title: string;
  description: string;
  group: string;
  keywords: string;
  order: number;
  body: string;
  /** Heading ids from the English page, so section links work in every language. */
  ids: string[];
  /** Where the page source lives, relative to the repository root. */
  path: string;
};

const EN = import.meta.glob<string>(['../../../../docs/guide/*.md', '!**/README.md'], {
  query: '?raw',
  import: 'default',
  eager: true,
});
const ES = import.meta.glob<string>(['../../../../docs/guide/es/*.md', '!**/README.md'], {
  query: '?raw',
  import: 'default',
  eager: true,
});

export const headingSlug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');

const outsideCode = (body: string) => body.replace(/^```[\s\S]*?^```/gm, '');

function parse(file: string, raw: string): Omit<DocPage, 'ids'> {
  const m = /^---\n([\s\S]*?)\n---\n/.exec(raw);
  const meta: Record<string, string> = {};
  for (const line of (m?.[1] ?? '').split('\n')) {
    const i = line.indexOf(':');
    if (i > 0) meta[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  const rest = raw.slice(m ? m[0].length : 0).trimStart();
  const h1 = /^# (.+)\n/.exec(rest);
  return {
    slug: file.split('/').pop()!.replace(/\.md$/, ''),
    title: h1?.[1] ?? '',
    description: meta.description ?? '',
    group: meta.group ?? '',
    keywords: meta.keywords ?? '',
    order: Number(meta.order ?? 0),
    body: h1 ? rest.slice(h1[0].length) : rest,
    path: file.replace(/^(\.\.\/)+/, ''),
  };
}

const english = Object.entries(EN).map(([f, raw]) => parse(f, raw));
const spanish = new Map(Object.entries(ES).map(([f, raw]) => [f.split('/').pop()!.replace(/\.md$/, ''), parse(f, raw)]));

function pages(lang: Lang): DocPage[] {
  return english
    .map((en) => {
      const ids = [...outsideCode(en.body).matchAll(/^#{2,3} (.+)$/gm)].map((h) => headingSlug(h[1]));
      const local = lang === 'es' ? spanish.get(en.slug) : undefined;
      return { ...(local ?? en), order: en.order, ids };
    })
    .sort((a, b) => a.order - b.order);
}

export const DOCS: Record<Lang, DocPage[]> = { en: pages('en'), es: pages('es') };
export const docGroups = (list: DocPage[]) => [...new Set(list.map((p) => p.group))];
