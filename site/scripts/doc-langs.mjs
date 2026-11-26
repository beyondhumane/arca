// Keeps the "English | Español" line under the title of every guide page.
// Add a language to LANGS, run `node scripts/doc-langs.mjs` and commit the result.
// `--check` fails when a page is out of date; the site build runs it.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const LANGS = [
  { label: "English", dir: "" },
  { label: "Español", dir: "es" },
];

const guide = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../docs/guide");
const check = process.argv.includes("--check");

const link = String.raw`(?:\*\*[^*\n]+\*\*|\[[^\]\n]+\]\([^)\n]+\))`;
const SWITCH = new RegExp(`^${link}(?: \\| ${link})+$`);

const files = (l) => {
  const dir = path.join(guide, l.dir);
  return fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => f.endsWith(".md")) : [];
};
const names = new Set(LANGS.flatMap(files));
let stale = 0;

for (const name of names) {
  const have = LANGS.filter((l) => files(l).includes(name));
  if (have.length < 2) continue;
  for (const l of have) {
    const file = path.join(guide, l.dir, name);
    const line = have
      .map((o) =>
        o === l
          ? `**${o.label}**`
          : `[${o.label}](${path.relative(path.dirname(file), path.join(guide, o.dir, name)).split(path.sep).join("/")})`,
      )
      .join(" | ");
    const lines = fs.readFileSync(file, "utf8").split("\n");
    const h1 = lines.findIndex((s) => s.startsWith("# "));
    if (h1 < 0) throw new Error(`${file}: no title`);
    if (lines[h1 + 1] === "" && SWITCH.test(lines[h1 + 2] ?? "")) lines[h1 + 2] = line;
    else lines.splice(h1 + 1, 0, "", line);
    const next = lines.join("\n");
    if (next === fs.readFileSync(file, "utf8")) continue;
    stale++;
    if (check) console.error(`${path.relative(guide, file)}: language line out of date`);
    else fs.writeFileSync(file, next);
  }
}
if (check && stale) {
  console.error("run `node scripts/doc-langs.mjs` in site/");
  process.exit(1);
}
