#!/usr/bin/env node
// Build first; --publish sends only the already-reviewed artifact to gh-pages.
import { execFileSync } from "node:child_process";
import {
  cp,
  mkdtemp,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const site = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repository = resolve(site, "..");
const output = join(site, ".pages");
const build = join(site, ".pages-build");
const base = "/omadesign/";
const required = [
  "index.html",
  "docs/index.html",
  "docs/manual/index.html",
  "docs/contributing/index.html",
  "docs/roadmap/index.html",
  "install",
];

function command(program, args, cwd = repository, extra = {}) {
  const output = execFileSync(program, args, {
    cwd,
    encoding: "utf8",
    ...extra,
  });
  return typeof output === "string" ? output.trim() : "";
}

async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (
    await Promise.all(
      entries.map(async (entry) => {
        const path = join(directory, entry.name);
        if (entry.name === ".git")
          throw new Error(`Build output contains a Git directory: ${path}`);
        if (entry.isSymbolicLink())
          throw new Error(`Build output contains a linked file: ${path}`);
        return entry.isDirectory() ? files(path) : [path];
      }),
    )
  ).flat();
}

async function validate(directory) {
  const paths = await files(directory);
  const names = new Set(
    paths.map((path) => relative(directory, path).split(sep).join("/")),
  );
  for (const name of required) {
    if (!names.has(name))
      throw new Error(`Missing prerendered output: ${name}`);
  }
  for (const path of paths) {
    if ((await stat(path)).size >= 100 * 1024 * 1024) {
      throw new Error(
        `File exceeds GitHub's 100 MB limit: ${relative(directory, path)}`,
      );
    }
    if (!path.endsWith(".html")) continue;
    const name = relative(directory, path).split(sep).join("/");
    const page = new URL(
      base + name.replace(/index\.html$/, ""),
      "https://pages.invalid",
    );
    const html = await readFile(path, "utf8");
    for (const match of html.matchAll(/\b(?:src|href)=["']([^"']+)["']/g)) {
      const value = match[1].replaceAll("&amp;", "&");
      if (/^(?:#|data:|mailto:|tel:|javascript:)/i.test(value)) continue;
      const url = new URL(value, page);
      if (url.origin !== page.origin) continue;
      if (!url.pathname.startsWith(base)) {
        throw new Error(`${name} links outside ${base}: ${value}`);
      }
      const target = decodeURIComponent(url.pathname.slice(base.length));
      const candidates = [target, `${target.replace(/\/$/, "")}/index.html`];
      if (!target) candidates.push("index.html");
      if (!candidates.some((candidate) => names.has(candidate))) {
        throw new Error(
          `${name} references a missing local page or asset: ${value}`,
        );
      }
    }
  }
  return names.size;
}

async function prepare() {
  await rm(build, { recursive: true, force: true });
  command(
    process.execPath,
    [join(site, "node_modules/vite/bin/vite.js"), "build"],
    site,
    {
      env: { ...process.env, GITHUB_PAGES: "1" },
      stdio: "inherit",
    },
  );
  const client = join(build, "client");
  const count = await validate(client);
  await rm(output, { recursive: true, force: true });
  await cp(client, output, { recursive: true });
  await writeFile(join(output, ".nojekyll"), "");
  await writeFile(
    join(output, "build-info.json"),
    JSON.stringify(
      {
        commit: command("git", ["rev-parse", "HEAD"]),
        builtAt: new Date().toISOString(),
        base,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    `Prepared ${count} files in ${output}. Review this build before publishing.`,
  );
  console.log(
    "Publish the reviewed artifact with: node site/scripts/publish-pages.mjs --publish",
  );
}

async function publish() {
  const info = JSON.parse(
    await readFile(join(output, "build-info.json"), "utf8"),
  );
  if (
    info.base !== base ||
    info.commit !== command("git", ["rev-parse", "HEAD"])
  ) {
    throw new Error(
      "The prepared site belongs to another revision. Build and review it again.",
    );
  }
  const changed = command("git", [
    "status",
    "--porcelain",
    "--",
    "site",
    "docs",
  ]);
  if (changed)
    throw new Error(
      "Commit website and documentation changes before preparing the published build.",
    );
  await validate(output);
  const remote = command("git", ["remote", "get-url", "origin"]);
  const repo = command("gh", [
    "repo",
    "view",
    "--json",
    "nameWithOwner",
    "--jq",
    ".nameWithOwner",
  ]);
  const pages = JSON.parse(command("gh", ["api", `repos/${repo}/pages`]));
  if (pages.build_type !== "legacy") {
    throw new Error(
      "This publisher expects GitHub Pages to deploy from a branch. Check the repository's Pages settings.",
    );
  }
  const temporary = await mkdtemp(join(tmpdir(), "omadesign-pages-"));
  try {
    command("git", ["init", "--quiet", "--initial-branch=gh-pages"], temporary);
    command("git", ["remote", "add", "origin", remote], temporary);
    const existing = command(
      "git",
      ["ls-remote", "--heads", "origin", "refs/heads/gh-pages"],
      temporary,
    );
    if (existing) {
      command(
        "git",
        ["fetch", "--quiet", "--depth=1", "origin", "gh-pages"],
        temporary,
      );
      command("git", ["reset", "--quiet", "--hard", "FETCH_HEAD"], temporary);
      for (const entry of await readdir(temporary)) {
        if (entry !== ".git")
          await rm(join(temporary, entry), { recursive: true, force: true });
      }
    }
    await cp(output, temporary, { recursive: true });
    command(
      "git",
      ["config", "user.name", command("git", ["config", "user.name"])],
      temporary,
    );
    command(
      "git",
      ["config", "user.email", command("git", ["config", "user.email"])],
      temporary,
    );
    command("git", ["add", "--all"], temporary);
    command(
      "git",
      [
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        `Publish website from ${info.commit.slice(0, 12)}`,
      ],
      temporary,
    );
    command("git", ["push", "origin", "HEAD:gh-pages"], temporary, {
      stdio: "inherit",
    });
    if (pages.source?.branch !== "gh-pages" || pages.source?.path !== "/") {
      command(
        "gh",
        [
          "api",
          `repos/${repo}/pages`,
          "--method",
          "PUT",
          "-f",
          "source[branch]=gh-pages",
          "-f",
          "source[path]=/",
        ],
        repository,
      );
      // The first push happened before Pages watched this branch. Request that
      // initial build explicitly; subsequent pushes trigger builds themselves.
      command("gh", ["api", `repos/${repo}/pages/builds`, "--method", "POST"]);
    }
    console.log(
      `Published generated branch gh-pages. GitHub Pages is building ${pages.html_url}`,
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

try {
  const args = process.argv.slice(2);
  if (!args.length) await prepare();
  else if (args.length === 1 && args[0] === "--publish") await publish();
  else
    throw new Error("Usage: node site/scripts/publish-pages.mjs [--publish]");
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
