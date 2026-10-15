#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';

const [mode, ...input] = process.argv.slice(2);
const inside = mode === '--inside';
const [configPath, repository, revision, ...args] = inside ? input : [mode, ...input];
if (!configPath || !repository || !/^[a-f0-9]{40}$/.test(revision ?? '') || !['test', 'check', 'clippy', 'fmt', 'metadata', 'build'].includes(args[0]) || args.includes('--target-dir')) {
  throw new Error('Usage: node scripts/serialized-cargo.mjs CONFIG REPOSITORY FULL_COMMIT test|check|clippy|fmt|metadata|build [ARGS]; target directory is centrally owned');
}
const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
const entry = config.repositories[repository];
if (!entry) throw new Error(`Unknown repository ${repository}`);
for (const directory of [config.scratch, entry.worktree, entry.target]) {
  if (!path.isAbsolute(directory) || !directory.startsWith('/home/')) throw new Error('Build paths must remain under /home');
}
if (!path.resolve(config.lock).startsWith(`${path.resolve(config.scratch)}/`)) throw new Error('Build lock must be in the shared scratch directory');
fs.mkdirSync(config.scratch, { recursive: true });
if (!inside) {
  console.error(`Waiting for the shared build lock: ${repository} ${revision.slice(0, 12)} cargo ${args[0]}`);
  const result = spawnSync('flock', [config.lock, process.execPath, fileURLToPath(import.meta.url), '--inside', configPath, repository, revision, ...args], { stdio: 'inherit' });
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}
/**
 * Read canonical build checkout state.
 * Args: Git arguments. Returns trimmed command output, or throws on failure.
 */
function git(...parameters) {
  return execFileSync('git', ['-C', entry.worktree, ...parameters], { encoding: 'utf8' }).trim();
}
if (git('status', '--porcelain', '--untracked-files=all')) throw new Error('Canonical build checkout has changes; preserve them and contact the coordinator');
if (git('rev-parse', `${revision}^{commit}`) !== revision) throw new Error('Revision is not an exact local commit');
execFileSync('git', ['-C', entry.worktree, 'checkout', '--quiet', '--detach', revision]);
const scratch = fs.mkdtempSync(path.join(config.scratch, 'cargo-'));
try {
  console.error(`Building ${repository} ${revision.slice(0, 12)} in ${entry.worktree}; shared target ${entry.target}`);
  const result = spawnSync('cargo', args, {
    cwd: entry.worktree,
    env: { ...process.env, CARGO_TARGET_DIR: entry.target, CARGO_BUILD_JOBS: '2', TMPDIR: scratch, TMP: scratch, TEMP: scratch },
    stdio: 'inherit',
  });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
} finally {
  fs.rmSync(scratch, { recursive: true, force: true });
}
