import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Download links must point at assets that exist, so the latest published
// release wins over Cargo.toml. CI writes it to ARCA_RELEASE_JSON with
// `gh release view --json tagName,assets`; local builds fall back to the
// workspace version and show no sizes.
function release(): { version: string; sizes: Record<string, number> } {
  const file = process.env.ARCA_RELEASE_JSON;
  if (file && fs.existsSync(file)) {
    let r;
    try {
      r = JSON.parse(fs.readFileSync(file, "utf8"));
    } catch (e) {
      throw new Error(`ARCA_RELEASE_JSON (${file}) is not valid JSON: ${e}`);
    }
    const sizes: Record<string, number> = {};
    for (const a of r.assets ?? []) sizes[a.name] = a.size;
    return { version: String(r.tagName).replace(/^v/, ""), sizes };
  }
  const cargo = fs.readFileSync(path.resolve(__dirname, "../Cargo.toml"), "utf8");
  const version = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1];
  if (!version) throw new Error("workspace.package.version not found in Cargo.toml");
  return { version, sizes: {} };
}

// CI reads the count with `gh api` so visitors never hit the rate-limited
// GitHub API; local builds show no count.
function stars(): number | null {
  const n = Number(process.env.ARCA_STARS);
  return Number.isInteger(n) && n >= 0 && process.env.ARCA_STARS !== "" ? n : null;
}

// Last commit date of every guide page, for "Last updated", sitemap lastmod
// and dateModified. Needs full history (fetch-depth: 0) to be accurate.
function docsUpdated(): Record<string, string> {
  const root = path.resolve(__dirname, "..");
  const out: Record<string, string> = {};
  for (const dir of ["docs/guide", "docs/guide/es"]) {
    for (const f of fs.readdirSync(path.join(root, dir))) {
      if (!f.endsWith(".md")) continue;
      const rel = `${dir}/${f}`;
      try {
        const d = execFileSync("git", ["log", "-1", "--format=%cs", "--", rel], { cwd: root, encoding: "utf8" }).trim();
        if (d) out[rel] = d;
      } catch {
        return {};
      }
    }
  }
  return out;
}

// https://vite.dev/config/
export default defineConfig({
  base: "/",
  define: {
    __ARCA_RELEASE__: JSON.stringify(release()),
    __ARCA_STARS__: JSON.stringify(stars()),
    __DOCS_UPDATED__: JSON.stringify(docsUpdated()),
  },
  plugins: [react(), tailwindcss()],
  server: { fs: { allow: [".."] } },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
});
