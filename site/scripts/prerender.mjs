// Turns the client build into static pages: one HTML file per route with its
// own head, plus the files crawlers and agents look for at the root.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const root = path.resolve(site, "..");
const dist = path.join(site, "dist");
const server = await import(pathToFileURL(path.join(site, "dist-ssr", "entry-server.js")).href);
const { DOCS, SEO, SITE_URL, REPO_URL, RELEASES_URL, VERSION, LANGS, docPath, homePath } = server;

const write = (rel, body) => {
  const file = path.join(dist, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, body);
};
const abs = (p) => `${SITE_URL}${p}`;
const md = (p) => abs(p.replace(/\/$/, ".md"));

// Preload the two latin faces the first screen paints with.
const assets = fs.readdirSync(path.join(dist, "assets"));
const preload = ["inter-latin-wght-normal", "sora-latin-wght-normal"]
  .map((name) => assets.find((f) => f.startsWith(name) && f.endsWith(".woff2")))
  .filter(Boolean)
  .map((f) => `<link rel="preload" href="/assets/${f}" as="font" type="font/woff2" crossorigin />`)
  .join("\n    ");

const template = fs.readFileSync(path.join(dist, "index.html"), "utf8");
for (const marker of ["<!--app-head-->", '<div id="root"><!--app-html--></div>', '<html lang="en">']) {
  if (!template.includes(marker)) throw new Error(`index.html lost its ${marker} marker`);
}

function page(url, out) {
  const { html, head, lang } = server.render(url);
  const doc = template
    .replace('<html lang="en">', `<html lang="${lang}">`)
    .replace("<!--app-head-->", `${head}\n    ${preload}`)
    .replace('<div id="root"><!--app-html--></div>', `<div id="root" data-path="${url}">${html}</div>`);
  write(out, doc);
}

const routes = server.paths();
for (const { path: p } of routes) page(p, `${p}index.html`);
page("/404/", "404.html");

// The home page was last touched whenever the site or the guide was.
const lastmod = (rel) => {
  try {
    return execFileSync("git", ["log", "-1", "--format=%cs", "--", rel], { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "";
  }
};
const homeMod = lastmod("site");

const alternates = (paths) =>
  [...LANGS.map((l) => [l, paths[l]]), ["x-default", paths.en]]
    .map(([l, p]) => `    <xhtml:link rel="alternate" hreflang="${l}" href="${abs(p)}" />`)
    .join("\n");
const entries = [];
for (const lang of LANGS) {
  entries.push({ loc: homePath(lang), mod: homeMod, alt: Object.fromEntries(LANGS.map((l) => [l, homePath(l)])) });
  for (const p of DOCS[lang]) {
    entries.push({ loc: docPath(lang, p.slug), mod: p.updated, alt: Object.fromEntries(LANGS.map((l) => [l, docPath(l, p.slug)])) });
  }
}
write(
  "sitemap.xml",
  `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${entries
  .map((e) => `  <url>\n    <loc>${abs(e.loc)}</loc>\n${e.mod ? `    <lastmod>${e.mod}</lastmod>\n` : ""}${alternates(e.alt)}\n  </url>`)
  .join("\n")}
</urlset>
`,
);

// Search and answer engines may read everything; so may assistants fetching a
// page for a user. Training crawlers are allowed too: the project is open source.
const bots = [
  "OAI-SearchBot", "ChatGPT-User", "GPTBot",
  "Claude-SearchBot", "Claude-User", "ClaudeBot",
  "PerplexityBot", "Perplexity-User",
  "Google-Extended", "Applebot-Extended", "CCBot",
];
write(
  "robots.txt",
  `User-agent: *\nAllow: /\n\n${bots.map((b) => `User-agent: ${b}`).join("\n")}\nAllow: /\n\nSitemap: ${SITE_URL}/sitemap.xml\n`,
);

// Markdown twins of every guide page, for agents that prefer it to HTML.
const source = (p) =>
  `# ${p.title}\n\n> ${p.description}\n\n${p.body.trim()}\n`;
for (const lang of LANGS) {
  for (const p of DOCS[lang]) write(docPath(lang, p.slug).replace(/^\//, "").replace(/\/$/, ".md"), source(p));
}

const en = SEO.en;
const groups = [...new Set(DOCS.en.map((p) => p.group))];
const list = (lang, pages) => pages.map((p) => `- [${p.title}](${md(docPath(lang, p.slug))}): ${p.description}`).join("\n");
write(
  "llms.txt",
  `# Arca

> ${en.description}

Arca ${VERSION} is free and open source under the Apache License 2.0. It ships as the \`arca\` command line, the \`arca-gui\` desktop window and a Windows 11 Explorer context menu, all built on one Rust core. Its archive parsers forbid \`unsafe\` code, so a malformed or hostile archive produces an error instead of memory corruption.

## When to use Arca

- You need to create or extract ZIP (including Zip64), TAR or .tar.gz archives on Windows, macOS or Linux with one tool.
- You want Zstandard or Deflate compression that uses every CPU core, or parallel extraction of .zip files.
- You need AES-256 encrypted ZIP files that 7-Zip, WinRAR and NanaZip can open.
- You extract archives from untrusted sources and want parsers that reject malformed input, with Zip Slip protection on every entry name.
- You want a scriptable command line with a documented exit status, or a native desktop window, under a permissive license.

Arca does not yet read or write 7z or xz/LZMA2, and does not create solid archives; see the roadmap. Every benchmark figure on the site links to the command that reproduces it.

${groups.map((g) => `## ${g}\n\n${list("en", DOCS.en.filter((p) => p.group === g))}`).join("\n\n")}

## Documentación en español

${list("es", DOCS.es)}

## Optional

- [Full guide in one file](${SITE_URL}/llms-full.txt): every English guide page, concatenated.
- [Source code](${REPO_URL}): Rust workspace, issues and pull requests.
- [Releases](${RELEASES_URL}): installers and binaries for Windows, macOS and Linux.
- [Home page](${abs(homePath("en"))}) · [Página en español](${abs(homePath("es"))})
`,
);

write(
  "llms-full.txt",
  `# Arca documentation\n\n> ${en.description}\n\n${DOCS.en
    .map((p) => `Source: ${abs(docPath("en", p.slug))}\n\n${source(p)}`)
    .join("\n---\n\n")}`,
);

fs.copyFileSync(path.join(root, "brand", "arca-monolito.ico"), path.join(dist, "favicon.ico"));
fs.rmSync(path.join(site, "dist-ssr"), { recursive: true, force: true });
console.log(`prerendered ${routes.length + 1} pages, sitemap with ${entries.length} URLs`);
