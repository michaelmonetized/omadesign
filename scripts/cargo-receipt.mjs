import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

/** Hash a file without loading it into memory. Args: filename. Returns SHA-256 and byte size. */
export async function hashFile(filename) {
  const hash = crypto.createHash('sha256');
  let size = 0;
  for await (const chunk of fs.createReadStream(filename)) {
    hash.update(chunk);
    size += chunk.length;
  }
  return { sha256: hash.digest('hex'), size };
}

/** Capture Cargo's executable artifacts while forwarding test output. Args: command, paths and environment. Returns command outcome and locked binary hashes. */
export async function runCargo(args, entry, env) {
  const separator = args.indexOf('--');
  const options = separator < 0 ? args : args.slice(0, separator);
  const structured = ['test', 'build'].includes(args[0]);
  if (structured && options.some(arg => arg === '--message-format' || arg.startsWith('--message-format='))) {
    throw new Error('Cargo message format is centrally owned for executable receipts');
  }
  const executed = structured ? [args[0], '--message-format=json-render-diagnostics', ...args.slice(1)] : args;
  const artifacts = new Map();
  const errors = [];
  let buildFinished = false;
  let spawnError;
  let interrupted;
  let result;
  const child = spawn('cargo', executed, { cwd: entry.worktree, env, stdio: ['inherit', 'pipe', 'inherit'] });
  const closed = new Promise(resolve => {
    child.once('error', error => { spawnError = error; });
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  const signals = new Map(['SIGINT', 'SIGTERM'].map(signal => [signal, () => {
    interrupted = signal;
    child.kill(signal);
  }]));
  for (const [signal, handler] of signals) process.on(signal, handler);
  try {
    if (child.stdout) {
      const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
      for await (const line of lines) {
        let message;
        if (structured && !buildFinished && line.startsWith('{')) {
          try { message = JSON.parse(line); } catch {}
        }
        if (message?.reason === 'compiler-artifact') {
          if (message.executable) artifacts.set(message.executable, {
            path: message.executable,
            package_id: message.package_id,
            target: message.target,
            profile: message.profile,
            fresh: message.fresh,
          });
        } else if (message?.reason === 'build-finished') {
          buildFinished = true;
        } else if (message?.reason === 'compiler-message') {
          if (message.message?.rendered) process.stderr.write(message.message.rendered);
        } else if (message?.reason === 'build-script-executed') {
          continue;
        } else {
          process.stdout.write(`${line}\n`);
        }
      }
    }
  } catch (error) {
    errors.push(`Cannot capture Cargo output: ${error.message}`);
    child.kill('SIGTERM');
  } finally {
    result = await closed;
    for (const [signal, handler] of signals) process.off(signal, handler);
  }
  if (spawnError) errors.push(`Cannot start Cargo: ${spawnError.message}`);
  const executables = [];
  for (const artifact of artifacts.values()) {
    try {
      const filename = fs.realpathSync(artifact.path);
      const relative = path.relative(fs.realpathSync(entry.target), filename);
      if (path.isAbsolute(relative) || relative === '..' || relative.startsWith(`..${path.sep}`)) {
        throw new Error('executable is outside the shared target');
      }
      if (!fs.statSync(filename).isFile()) throw new Error('executable is not a file');
      executables.push({ ...artifact, path: filename, ...await hashFile(filename) });
    } catch (error) {
      errors.push(`Cannot bind executable ${artifact.path}: ${error.message}`);
    }
  }
  const signal = result.signal ?? interrupted ?? null;
  return {
    executed_cargo_args: executed,
    message_format: structured ? 'json-render-diagnostics' : null,
    exit_code: signal ? 128 + (os.constants.signals[signal] ?? 1) : result.code ?? 1,
    signal,
    executables,
    receipt_errors: errors,
  };
}

/** Persist an immutable receipt before releasing the lock. Args: receipt directory and record. Returns the content-addressed filename. */
export function saveReceipt(directory, record) {
  fs.mkdirSync(directory, { recursive: true });
  const body = `${JSON.stringify(record, null, 2)}\n`;
  const digest = crypto.createHash('sha256').update(body).digest('hex');
  const filename = path.join(directory, `${digest}.json`);
  const fd = fs.openSync(filename, 'wx', 0o600);
  try {
    fs.writeFileSync(fd, body);
    fs.fsyncSync(fd);
  } finally {
    fs.closeSync(fd);
  }
  return filename;
}
