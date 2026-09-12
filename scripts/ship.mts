#!/usr/bin/env bun
/** Dispatches the Blacksmith/Vercel prebuilt ship workflow for omadesign.app. */
import { readFile } from "node:fs/promises";
import { parseArgs } from "node:util";

type RunOptions = { capture?: boolean };

const { values } = parseArgs({
  args: process.argv.slice(2),
  options: {
    branch: { type: "string", short: "b" },
    help: { type: "boolean", short: "h" },
    "no-watch": { type: "boolean" },
    setup: { type: "boolean" },
  },
});

if (values.help) {
  console.log("Usage: bun scripts/ship.mts [--setup] [--branch BRANCH] [--no-watch]");
  process.exit(0);
}

async function run(cmd: string, args: string[], opts: RunOptions = {}) {
  const proc = Bun.spawn([cmd, ...args], {
    cwd: process.cwd(),
    env: process.env,
    stdout: opts.capture ? "pipe" : "inherit",
    stderr: opts.capture ? "pipe" : "inherit",
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    opts.capture ? new Response(proc.stdout).text() : "",
    opts.capture ? new Response(proc.stderr).text() : "",
    proc.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(stderr.trim() || `${cmd} ${args.join(" ")} exited ${exitCode}`);
  }
  return stdout.trim();
}

async function repoName() {
  return run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"], {
    capture: true,
  });
}

async function branchName() {
  return values.branch ?? run("git", ["branch", "--show-current"], { capture: true });
}

async function readProject() {
  for (const path of ["site/.vercel/project.json", ".vercel/project.json"]) {
    try {
      return JSON.parse(await readFile(path, "utf8")) as {
        orgId: string;
        projectId: string;
      };
    } catch {
      // try the next path
    }
  }
  throw new Error("Link the site first: vercel link --cwd site --yes --project omadesign --scope hustle-launch");
}

async function setupSecrets(repo: string) {
  const project = await readProject();
  await run("gh", ["secret", "set", "VERCEL_ORG_ID", "--repo", repo, "--body", project.orgId]);
  await run("gh", [
    "secret",
    "set",
    "VERCEL_PROJECT_ID",
    "--repo",
    repo,
    "--body",
    project.projectId,
  ]);
  if (process.env.VERCEL_TOKEN) {
    await run("gh", [
      "secret",
      "set",
      "VERCEL_TOKEN",
      "--repo",
      repo,
      "--body",
      process.env.VERCEL_TOKEN,
    ]);
  }
}

const repo = await repoName();
if (values.setup) await setupSecrets(repo);
const branch = await branchName();
await run("git", ["push", "origin", branch]);
await run("gh", ["workflow", "run", "ship.yml", "--repo", repo, "--ref", branch]);
if (!values["no-watch"]) await run("gh", ["run", "watch", "--repo", repo, "--exit-status"]);
