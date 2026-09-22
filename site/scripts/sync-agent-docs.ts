import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Publish the same source files embedded in the release, without stale copies. */
export function syncAgentDocs() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  const files = [
    ["docs/MANUAL.md", "docs/markdown/manual.md"],
    ["docs/layout.md", "docs/markdown/layout.md"],
    ["docs/format-support.md", "docs/markdown/formats.md"],
    ["docs/plugins.md", "docs/markdown/plugins.md"],
    ["docs/cloud.md", "docs/markdown/cloud.md"],
    ["docs/CONTRIBUTING.md", "docs/markdown/contributing.md"],
    ["skills/omadesign-create/SKILL.md", "skills/omadesign-create/SKILL.md"],
  ];
  const manifest = files.map(([source, destination]) => {
    const input = resolve(root, source);
    const output = resolve(root, "site/public", destination);
    mkdirSync(dirname(output), { recursive: true });
    copyFileSync(input, output);
    return { path: `/${destination}`, sha256: createHash("sha256").update(readFileSync(input)).digest("hex") };
  });
  const version = readFileSync(resolve(root, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)?.[1];
  if (!version) throw new Error("Cannot determine the documentation release version");
  writeFileSync(resolve(root, "site/public/docs/markdown/manifest.json"), `${JSON.stringify({ version, files: manifest }, null, 2)}\n`);
}
