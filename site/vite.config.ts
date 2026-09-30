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

// https://vite.dev/config/
export default defineConfig({
  base: "./",
  define: { __ARCA_RELEASE__: JSON.stringify(release()) },
  plugins: [react(), tailwindcss()],
  server: { fs: { allow: [".."] } },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
});
