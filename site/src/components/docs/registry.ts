export type DocPage = {
  slug: string;
  title: string;
  description: string;
  group: string;
  keywords: string;
  body: string;
};

const FILES = import.meta.glob<string>(['../../../../docs/guide/*.md', '!**/README.md'], {
  query: '?raw',
  import: 'default',
  eager: true,
});

function parse(path: string, raw: string): DocPage & { order: number } {
  const m = /^---\n([\s\S]*?)\n---\n/.exec(raw);
  const meta: Record<string, string> = {};
  for (const line of (m?.[1] ?? '').split('\n')) {
    const i = line.indexOf(':');
    if (i > 0) meta[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  const rest = raw.slice(m ? m[0].length : 0).trimStart();
  const h1 = /^# (.+)\n/.exec(rest);
  return {
    slug: path.split('/').pop()!.replace(/\.md$/, ''),
    title: h1?.[1] ?? '',
    description: meta.description ?? '',
    group: meta.group ?? '',
    keywords: meta.keywords ?? '',
    order: Number(meta.order ?? 0),
    body: h1 ? rest.slice(h1[0].length) : rest,
  };
}

export const DOC_PAGES: DocPage[] = Object.entries(FILES)
  .map(([path, raw]) => parse(path, raw))
  .sort((a, b) => a.order - b.order);

export const DOC_GROUPS = [...new Set(DOC_PAGES.map((p) => p.group))];
