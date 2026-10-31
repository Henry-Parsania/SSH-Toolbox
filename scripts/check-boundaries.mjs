// Fails if any file outside src/lib/backend/ imports @tauri-apps/* (PLAN.md design rule 3).
import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcDir = join(root, "src");
const allowedDir = join(srcDir, "lib", "backend") + sep;
const extensions = [".ts", ".js", ".mjs", ".svelte", ".tsx", ".jsx"];
// Matches `from "@tauri-apps/..."`, `import "@tauri-apps/..."`, `import("@tauri-apps/...")` and `require("@tauri-apps/...")`.
const pattern = /(?:from|import|require)\s*\(?\s*["'`]@tauri-apps\//;

function* walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else if (extensions.some((ext) => entry.name.endsWith(ext))) yield full;
  }
}

const violations = [];
for (const file of walk(srcDir)) {
  if (file.startsWith(allowedDir)) continue;
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line, i) => {
    if (pattern.test(line)) violations.push(`${relative(root, file)}:${i + 1}: ${line.trim()}`);
  });
}

if (violations.length > 0) {
  console.error("Only src/lib/backend/ may import @tauri-apps/*. Found:");
  for (const v of violations) console.error("  " + v);
  process.exit(1);
}
console.log("Boundary check passed.");
