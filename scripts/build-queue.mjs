import fs from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';

const ticketPattern = /^\d{20}-[a-f0-9-]{36}\.ticket$/;
const pendingPattern = /^pending-[a-f0-9-]{36}$/;

/** Lock an inherited descriptor. Args: descriptor, nonblocking mode and shared mode. Returns whether the kernel granted ownership. */
function lockDescriptor(fd, nonblocking = false, shared = false) {
  const result = spawnSync('flock', [...(nonblocking ? ['--nonblock'] : []), shared ? '--shared' : '--exclusive', '3'], {
    stdio: ['ignore', 'ignore', 'pipe', fd], encoding: 'utf8',
  });
  if (result.error) throw result.error;
  if (nonblocking && result.status === 1) return false;
  if (result.status !== 0) throw new Error(`Cannot lock build queue: ${result.stderr || result.signal || result.status}`);
  return true;
}

/** Serialize a short queue edit. Args: queue directory and synchronous edit. Returns the edit's result after unlocking. */
function editQueue(directory, edit) {
  const fd = fs.openSync(path.join(directory, '.lock'), 'a', 0o600);
  try {
    lockDescriptor(fd);
    return edit();
  } finally {
    fs.closeSync(fd);
  }
}

/** Remove a released ticket. Args: ticket path. Returns false while any owner or inherited child retains its descriptor. */
function removeReleased(filename) {
  let fd;
  try {
    fd = fs.openSync(filename, 'r');
    if (!lockDescriptor(fd, true, true)) return false;
    fs.unlinkSync(filename);
    return true;
  } catch (error) {
    if (error.code === 'ENOENT') return true;
    throw error;
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

/** Find the first owned ticket. Args: queue directory with its metadata lock held. Returns its name, clearing abandoned heads. */
function firstTicket(directory) {
  for (const name of fs.readdirSync(directory).filter(name => ticketPattern.test(name)).sort()) {
    if (!removeReleased(path.join(directory, name))) return name;
  }
}

/** Register one submission atomically. Args: queue directory. Returns its unique ticket path and locked descriptor. */
function register(directory) {
  const token = randomUUID();
  let filename = path.join(directory, `pending-${token}`);
  const fd = fs.openSync(filename, 'wx', 0o600);
  try {
    lockDescriptor(fd);
    fs.writeFileSync(fd, `${JSON.stringify({ pid: process.pid, token })}\n`);
    editQueue(directory, () => {
      for (const name of fs.readdirSync(directory).filter(name => pendingPattern.test(name))) {
        removeReleased(path.join(directory, name));
      }
      firstTicket(directory);
      const tickets = fs.readdirSync(directory).filter(name => ticketPattern.test(name)).sort();
      const sequence = (tickets.length ? BigInt(tickets.at(-1).slice(0, 20)) : 0n) + 1n;
      if (sequence.toString().length > 20) throw new Error('Build queue ticket sequence exhausted');
      const published = path.join(directory, `${sequence.toString().padStart(20, '0')}-${token}.ticket`);
      fs.renameSync(filename, published);
      filename = published;
    });
    return { filename, fd };
  } catch (error) {
    fs.closeSync(fd);
    editQueue(directory, () => removeReleased(filename));
    throw error;
  }
}

/** Wait for the head's kernel ownership to end. Args: ticket path and optional cancellation signal. Returns after its lock waiter closes. */
async function waitForTicket(filename, signal) {
  let fd;
  try {
    fd = fs.openSync(filename, 'r');
  } catch (error) {
    if (error.code === 'ENOENT') return;
    throw error;
  }
  const child = spawn('flock', ['--shared', '3'], { stdio: ['ignore', 'ignore', 'pipe', fd] });
  fs.closeSync(fd);
  let failure;
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
  const closed = new Promise(resolve => {
    child.once('error', error => { failure = error; });
    child.once('close', (code, childSignal) => resolve({ code, signal: childSignal }));
  });
  const abort = () => child.kill('SIGTERM');
  signal?.addEventListener('abort', abort, { once: true });
  if (signal?.aborted) abort();
  try {
    const result = await closed;
    signal?.throwIfAborted();
    if (failure) throw failure;
    if (result.code !== 0) throw new Error(`Cannot wait for build queue: ${stderr || result.signal || result.code}`);
  } finally {
    signal?.removeEventListener('abort', abort);
  }
}

/** Run a future build submission in registration order. Args: heavy lock path, async callback receiving ticketFd, optional AbortSignal. Returns the callback's result; inherit ticketFd into the locked child and await its close. */
export async function withBuildQueue(lockPath, run, { signal } = {}) {
  signal?.throwIfAborted();
  const directory = `${path.resolve(lockPath)}.queue`;
  if (!directory.startsWith('/home/')) throw new Error('Build queue must remain under /home');
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  const ticket = register(directory);
  try {
    while (true) {
      signal?.throwIfAborted();
      const head = editQueue(directory, () => firstTicket(directory));
      if (head === path.basename(ticket.filename)) return await run({ ticketFd: ticket.fd });
      if (!head) throw new Error('Build queue lost its owned ticket');
      await waitForTicket(path.join(directory, head), signal);
    }
  } finally {
    fs.closeSync(ticket.fd);
    editQueue(directory, () => {
      removeReleased(ticket.filename);
      firstTicket(directory);
    });
  }
}
