#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawn } from 'node:child_process';
import { hashFile, runCargo, saveReceipt } from './cargo-receipt.mjs';
import { withBuildQueue } from './build-queue.mjs';

const [mode, ...input] = process.argv.slice(2);
const inside = mode === '--inside';
const [configPath, repository, revision, ...args] = inside ? input : [mode, ...input];
if (!configPath || !repository || !/^[a-f0-9]{40}$/.test(revision ?? '') || !['test', 'check', 'clippy', 'fmt', 'metadata', 'build'].includes(args[0]) || args.some(arg => arg === '--target-dir' || arg.startsWith('--target-dir='))) {
  throw new Error('Usage: node scripts/serialized-cargo.mjs CONFIG REPOSITORY FULL_COMMIT test|check|clippy|fmt|metadata|build [ARGS]; target directory is centrally owned');
}
const separator = args.indexOf('--');
if (['test', 'build'].includes(args[0]) && (separator < 0 ? args : args.slice(0, separator)).some(arg => arg === '--message-format' || arg.startsWith('--message-format='))) {
  throw new Error('Cargo message format is centrally owned for executable receipts');
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
  const controller = new AbortController();
  let interrupted;
  const signals = new Map(['SIGINT', 'SIGTERM'].map(signal => [signal, () => {
    interrupted ??= signal;
    controller.abort();
  }]));
  for (const [signal, handler] of signals) process.on(signal, handler);
  try {
    const result = await withBuildQueue(config.lock, async ({ ticketFd }) => {
      controller.signal.throwIfAborted();
      const child = spawn('flock', ['--no-fork', config.lock, process.execPath, fileURLToPath(import.meta.url), '--inside', configPath, repository, revision, ...args], {
        stdio: ['inherit', 'inherit', 'inherit', ticketFd],
      });
      let failure;
      const closed = new Promise(resolve => {
        child.once('error', error => { failure = error; });
        child.once('close', (code, signal) => resolve({ code, signal }));
      });
      const abort = () => child.kill(interrupted);
      controller.signal.addEventListener('abort', abort, { once: true });
      if (controller.signal.aborted) abort();
      try {
        const outcome = await closed;
        if (failure) throw failure;
        return outcome;
      } finally {
        controller.signal.removeEventListener('abort', abort);
      }
    }, { signal: controller.signal });
    process.exitCode = interrupted ? 128 + os.constants.signals[interrupted] : result.code ?? (128 + (os.constants.signals[result.signal] ?? 1));
  } catch (error) {
    if (!controller.signal.aborted) throw error;
    process.exitCode = 128 + os.constants.signals[interrupted];
  } finally {
    for (const [signal, handler] of signals) process.off(signal, handler);
  }
  process.exit(process.exitCode);
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
  const started = new Date().toISOString();
  const sourceTree = git('rev-parse', `${revision}^{tree}`);
  const helperHash = (await hashFile(fileURLToPath(import.meta.url))).sha256;
  const receiptHash = (await hashFile(fileURLToPath(new URL('./cargo-receipt.mjs', import.meta.url)))).sha256;
  const queueHash = (await hashFile(fileURLToPath(new URL('./build-queue.mjs', import.meta.url)))).sha256;
  const configHash = (await hashFile(configPath)).sha256;
  const result = await runCargo(args, entry, { ...process.env, CARGO_TARGET_DIR: entry.target, CARGO_BUILD_JOBS: '2', TMPDIR: scratch, TMP: scratch, TEMP: scratch });
  const dirtyAfter = Boolean(git('status', '--porcelain', '--untracked-files=all'));
  if (dirtyAfter) result.receipt_errors.push('Canonical build checkout changed during Cargo; preserve it and contact the coordinator');
  const receipt = saveReceipt(path.join(config.scratch, 'receipts', repository, revision), {
    schema: 1,
    repository,
    revision,
    source_tree: sourceTree,
    dirty_before: false,
    dirty_after: dirtyAfter,
    cargo_args: args,
    ...result,
    started_at: started,
    finished_at: new Date().toISOString(),
    helper_sha256: helperHash,
    receipt_helper_sha256: receiptHash,
    queue_helper_sha256: queueHash,
    config_sha256: configHash,
  });
  console.error(`Cargo receipt: ${receipt}`);
  process.exitCode = result.exit_code || (result.receipt_errors.length ? 1 : 0);
} finally {
  fs.rmSync(scratch, { recursive: true, force: true });
}
