import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const helper = path.join(root, 'scripts/serialized-cargo.mjs');
const receiptHelper = path.join(root, 'scripts/cargo-receipt.mjs');
const queueHelper = path.join(root, 'scripts/build-queue.mjs');
const messageFormat = '--message-format=json-render-diagnostics';
const ordinaryOutput = 'running 1 test\n{"reason":"compiler-artifact","executable":"ordinary test stdout"}\ntest cached_case ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n';
const target = { name: 'receipt-fixture', kind: ['test'] };
const profile = { test: true };
const mockCargo = String.raw`
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { setTimeout: delay } = require('node:timers/promises');

(async () => {
  const state = process.env.FIXTURE_STATE;
  const args = process.argv.slice(2);
  const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const marker = fs.readFileSync('source.txt', 'utf8').trim();
  const executable = path.join(process.env.CARGO_TARGET_DIR, 'debug/deps/receipt-fixture');
  const mode = process.env.FAKE_CARGO_MODE || 'artifact';
  const job = process.env.FAKE_JOB_ID || marker;
  if (process.env.FAKE_DELAY_SIGNAL === '1') {
    for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => {
      fs.writeFileSync(path.join(state, 'signal-' + job), signal);
    });
  }
  const guard = path.join(state, 'cargo-active');
  const fd = fs.openSync(guard, 'wx');
  fs.closeSync(fd);
  try {
    if (process.env.FAKE_PRIOR_REVISION) {
      const directory = path.join(state, 'scratch/receipts/fixture', process.env.FAKE_PRIOR_REVISION);
      const names = fs.readdirSync(directory);
      if (names.length !== 1) throw new Error('First receipt was not persisted before the next Cargo invocation');
      const body = fs.readFileSync(path.join(directory, names[0]), 'utf8');
      const receipt = JSON.parse(body);
      const binaryHash = crypto.createHash('sha256').update(fs.readFileSync(executable)).digest('hex');
      if (receipt.executables[0]?.sha256 !== binaryHash) throw new Error('First receipt does not bind the executable before overwrite');
      fs.writeFileSync(path.join(state, 'prior-receipt.json'), JSON.stringify({ name: names[0], body }));
    }
    fs.appendFileSync(path.join(state, 'order'), job + '\n');
    fs.writeFileSync(path.join(state, 'started-' + job + '.json'), JSON.stringify({
      args, revision, marker, executable,
      target: process.env.CARGO_TARGET_DIR,
      jobs: process.env.CARGO_BUILD_JOBS,
      temporary: [process.env.TMPDIR, process.env.TMP, process.env.TEMP],
    }));
    if (process.env.FAKE_WAIT_FOR_RELEASE === '1') {
      const deadline = Date.now() + 8000;
      while (!fs.existsSync(path.join(state, process.env.FAKE_RELEASE_FILE || 'release'))) {
        if (Date.now() > deadline) throw new Error('Timed out waiting for fixture release');
        await delay(10);
      }
      if (fs.readFileSync('source.txt', 'utf8').trim() !== marker) throw new Error('Canonical checkout changed while Cargo held the lock');
    }
    if (mode !== 'empty' && mode !== 'fail') {
      if (mode !== 'cached' && mode !== 'missing') {
        fs.mkdirSync(path.dirname(executable), { recursive: true });
        fs.writeFileSync(executable, 'binary:' + marker + '\n', { mode: 0o700 });
      }
      const artifact = JSON.stringify({
        reason: 'compiler-artifact',
        package_id: 'receipt-fixture 0.0.0 (path+file://' + process.cwd() + ')',
        manifest_path: path.join(process.cwd(), 'Cargo.toml'),
        target: { name: 'receipt-fixture', kind: ['test'] },
        profile: { test: true },
        executable,
        fresh: true,
      }) + '\n';
      process.stdout.write(artifact.slice(0, 43));
      await delay(5);
      process.stdout.write(artifact.slice(43));
    }
    process.stdout.write(JSON.stringify({ reason: 'build-finished', success: mode !== 'fail' }) + '\n');
    if (mode === 'fail') {
      process.stderr.write('error: fake Cargo failure\n');
      process.exitCode = 37;
    } else if (args[0] === 'test' && !args.includes('--no-run')) {
      process.stdout.write(process.env.FAKE_TEST_OUTPUT);
    } else {
      process.stdout.write('Finished fake build\n');
    }
  } finally {
    fs.unlinkSync(guard);
  }
})().catch(error => { console.error(error); process.exitCode = 91; });
`;

/**
 * Hash fixture bytes.
 * Args: a string or buffer. Returns its lowercase SHA-256 digest.
 */
function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/**
 * Read fixture Git state.
 * Args: fixture and Git arguments. Returns trimmed output or throws on failure.
 */
function git(fixture, ...args) {
  return execFileSync('git', ['-C', fixture.worktree, ...args], {
    env: fixture.env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
}

/**
 * Stop an owned helper process group.
 * Args: the spawned child. Returns nothing; an already stopped group is harmless.
 */
function stop(child) {
  if (!child.pid) return;
  try {
    process.kill(-child.pid, 'SIGKILL');
  } catch (error) {
    if (error.code !== 'ESRCH') throw error;
  }
}

/**
 * Isolate one functional test.
 * Args: an async fixture callback. Returns its result and always removes its files.
 */
async function withFixture(run) {
  const artifacts = path.join(root, '.artifacts');
  fs.mkdirSync(artifacts, { recursive: true });
  const directory = fs.mkdtempSync(path.join(artifacts, 'serialized-cargo-tests-'));
  const fixture = {
    directory,
    worktree: path.join(directory, 'canonical'),
    target: path.join(directory, 'target'),
    scratch: path.join(directory, 'scratch'),
    config: path.join(directory, 'config.json'),
    children: new Set(),
  };
  fixture.executable = path.join(fixture.target, 'debug/deps/receipt-fixture');
  fixture.env = {
    ...process.env,
    PATH: path.join(directory, 'bin') + path.delimiter + process.env.PATH,
    TMPDIR: directory, TMP: directory, TEMP: directory,
    CARGO_HOME: path.join(directory, 'cargo-home'),
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_AUTHOR_NAME: 'Receipt Test', GIT_AUTHOR_EMAIL: 'receipt@example.invalid',
    GIT_COMMITTER_NAME: 'Receipt Test', GIT_COMMITTER_EMAIL: 'receipt@example.invalid',
    FIXTURE_STATE: directory,
    FAKE_TEST_OUTPUT: ordinaryOutput,
  };
  try {
    assert.ok(directory.startsWith('/home/'));
    for (const name of ['bin', 'canonical', 'scratch', 'target']) fs.mkdirSync(path.join(directory, name));
    fs.writeFileSync(path.join(directory, 'bin/cargo'), '#!' + process.execPath + '\n' + mockCargo, { mode: 0o700 });
    fs.writeFileSync(fixture.config, JSON.stringify({
      scratch: fixture.scratch,
      lock: path.join(fixture.scratch, 'build.lock'),
      repositories: { fixture: { worktree: fixture.worktree, target: fixture.target } },
    }, null, 2) + '\n');
    git(fixture, 'init', '--quiet', '--template=');
    fs.writeFileSync(path.join(fixture.worktree, 'Cargo.toml'), '[package]\nname = "receipt-fixture"\nversion = "0.0.0"\n');
    fixture.revisions = [];
    for (const marker of ['A', 'B']) {
      fs.writeFileSync(path.join(fixture.worktree, 'source.txt'), marker + '\n');
      git(fixture, 'add', 'Cargo.toml', 'source.txt');
      git(fixture, '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'Fixture ' + marker);
      fixture.revisions.push(git(fixture, 'rev-parse', 'HEAD'));
    }
    return await run(fixture);
  } finally {
    for (const child of fixture.children) stop(child.process);
    await Promise.allSettled([...fixture.children].map(child => child.done));
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

/**
 * Start the real serialized helper.
 * Args: fixture, revision, Cargo arguments and mock environment. Returns captured streams and completion.
 */
function launch(fixture, revision, args, env = {}) {
  return launchOwned(fixture, process.execPath, [helper, fixture.config, 'fixture', revision, ...args], env);
}

/** Start a fixture-owned process group. Args: fixture, executable, arguments and environment. Returns streams and close outcome. */
function launchOwned(fixture, executable, args, env = {}) {
  const child = spawn(executable, args, {
    cwd: root, env: { ...fixture.env, ...env }, detached: true, stdio: ['ignore', 'pipe', 'pipe'],
  });
  const state = { process: child, stdout: '', stderr: '' };
  fixture.children.add(state);
  child.stdout.setEncoding('utf8').on('data', chunk => { state.stdout += chunk; });
  child.stderr.setEncoding('utf8').on('data', chunk => { state.stderr += chunk; });
  const timer = setTimeout(() => stop(child), 12000);
  state.done = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code, signal) => {
      clearTimeout(timer);
      fixture.children.delete(state);
      resolve({ code, signal, stdout: state.stdout, stderr: state.stderr });
    });
  });
  return state;
}

/** Read published queue ownership. Args: fixture. Returns ordered names and informational owner PIDs. */
function tickets(fixture) {
  const directory = path.join(fixture.scratch, 'build.lock.queue');
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory).filter(name => /^\d{20}-.+\.ticket$/.test(name)).sort().map(name => ({
    name, ...JSON.parse(fs.readFileSync(path.join(directory, name), 'utf8')),
  }));
}

/** Observe atomic ticket publication. Args: fixture and submission. Returns after that submission is registered. */
async function queued(fixture, child) {
  await waitFor(() => tickets(fixture).some(ticket => ticket.pid === child.process.pid), 'ticket registration');
}

/** Probe one fixture lock without waiting. Args: fixture and lock path. Returns the real flock exit status. */
function probeLock(fixture, filename) {
  const result = spawnSync('flock', ['--nonblock', filename, process.execPath, '-e', 'process.exit(0)'], { env: fixture.env });
  assert.ifError(result.error);
  return result.status;
}

/** Hold the fixture heavy lock as a legacy caller. Args: fixture. Returns an owned process that exits only after a release marker. */
function legacyHolder(fixture) {
  return launchOwned(fixture, 'flock', ['--no-fork', path.join(fixture.scratch, 'build.lock'), process.execPath, '-e', `
    const fs = require('node:fs');
    const path = require('node:path');
    const { setTimeout: delay } = require('node:timers/promises');
    (async () => {
      const state = process.env.FIXTURE_STATE;
      fs.writeFileSync(path.join(state, 'legacy-started'), 'held');
      while (!fs.existsSync(path.join(state, 'legacy-release'))) await delay(10);
    })().catch(error => { console.error(error); process.exitCode = 1; });
  `]);
}

/**
 * Wait for an observable queue checkpoint.
 * Args: predicate and checkpoint name. Returns when true or throws after five seconds.
 */
async function waitFor(predicate, checkpoint) {
  const deadline = Date.now() + 5000;
  while (!predicate()) {
    assert.ok(Date.now() < deadline, 'Timed out waiting for ' + checkpoint);
    await delay(10);
  }
}

/**
 * Verify a persisted receipt against independent fixture state.
 * Args: fixture, helper outcome, revision and original arguments. Returns record, path and original bytes.
 */
function readReceipt(fixture, outcome, revision, args, signal = null) {
  const matches = [...outcome.stderr.matchAll(/^Cargo receipt: (.+)$/gm)];
  assert.equal(matches.length, 1, outcome.stderr);
  const filename = matches[0][1];
  assert.ok(path.isAbsolute(filename));
  assert.equal(path.dirname(filename), path.join(fixture.scratch, 'receipts/fixture', revision));
  const body = fs.readFileSync(filename);
  assert.equal(path.basename(filename), sha256(body) + '.json');
  const record = JSON.parse(body);
  assert.deepEqual(Object.keys(record).sort(), [
    'schema', 'repository', 'revision', 'source_tree', 'dirty_before', 'dirty_after',
    'cargo_args', 'executed_cargo_args', 'message_format', 'exit_code', 'signal',
    'started_at', 'finished_at', 'helper_sha256', 'receipt_helper_sha256', 'queue_helper_sha256', 'config_sha256',
    'executables', 'receipt_errors',
  ].sort());
  assert.equal(record.schema, 1);
  assert.equal(record.repository, 'fixture');
  assert.equal(record.revision, revision);
  assert.equal(record.source_tree, git(fixture, 'rev-parse', revision + '^{tree}'));
  assert.equal(record.dirty_before, false);
  assert.equal(record.dirty_after, false);
  assert.deepEqual(record.cargo_args, args);
  assert.equal(record.message_format, 'json-render-diagnostics');
  assert.equal(record.executed_cargo_args.filter(arg => arg === messageFormat).length, 1);
  assert.deepEqual(record.executed_cargo_args.filter(arg => arg !== messageFormat), args);
  const separator = record.executed_cargo_args.indexOf('--');
  assert.ok(separator < 0 || record.executed_cargo_args.indexOf(messageFormat) < separator);
  assert.equal(record.signal, signal);
  assert.equal(record.helper_sha256, sha256(fs.readFileSync(helper)));
  assert.equal(record.receipt_helper_sha256, sha256(fs.readFileSync(receiptHelper)));
  assert.equal(record.queue_helper_sha256, sha256(fs.readFileSync(queueHelper)));
  assert.equal(record.config_sha256, sha256(fs.readFileSync(fixture.config)));
  assert.ok(Number.isFinite(Date.parse(record.started_at)));
  assert.ok(Date.parse(record.finished_at) >= Date.parse(record.started_at));
  assert.ok(Array.isArray(record.executables));
  assert.ok(Array.isArray(record.receipt_errors));
  return { record, filename, body };
}

/**
 * Verify one streamed executable binding.
 * Args: fixture, receipt and expected binary bytes. Returns nothing or throws on mismatch.
 */
function assertExecutable(fixture, record, bytes) {
  assert.deepEqual(record.executables, [{
    path: fixture.executable,
    sha256: sha256(bytes),
    size: Buffer.byteLength(bytes),
    package_id: 'receipt-fixture 0.0.0 (path+file://' + fixture.worktree + ')',
    target, profile, fresh: true,
  }]);
}

test('cached streamed artifact binds the exact binary, source and arguments while preserving test stdout', async () => {
  await withFixture(async fixture => {
    fs.mkdirSync(path.dirname(fixture.executable), { recursive: true });
    fs.writeFileSync(fixture.executable, 'cached binary:A\n', { mode: 0o700 });
    const args = ['test', '--lib', 'cached_case', '--', '--exact', '--nocapture'];
    const outcome = await launch(fixture, fixture.revisions[0], args, { FAKE_CARGO_MODE: 'cached' }).done;
    assert.equal(outcome.code, 0, outcome.stderr);
    assert.equal(outcome.stdout, ordinaryOutput);
    const { record } = readReceipt(fixture, outcome, fixture.revisions[0], args);
    assert.equal(record.exit_code, 0);
    assert.deepEqual(record.receipt_errors, []);
    assertExecutable(fixture, record, 'cached binary:A\n');
    const invocation = JSON.parse(fs.readFileSync(path.join(fixture.directory, 'started-A.json')));
    assert.deepEqual(invocation.args, record.executed_cargo_args);
    assert.equal(invocation.revision, fixture.revisions[0]);
    assert.equal(invocation.target, fixture.target);
    assert.equal(invocation.jobs, '2');
    for (const temporary of invocation.temporary) {
      assert.equal(path.dirname(temporary), fixture.scratch);
      assert.equal(fs.existsSync(temporary), false);
    }
    assert.equal(git(fixture, 'status', '--porcelain', '--untracked-files=all'), '');
  });
});

test('queued commits retain separate receipts before replacing the same target executable', async () => {
  await withFixture(async fixture => {
    const firstArgs = ['build', '--profile', 'receipt-test'];
    const secondArgs = ['test', '--no-run', '--profile', 'receipt-test'];
    const first = launch(fixture, fixture.revisions[0], firstArgs, { FAKE_WAIT_FOR_RELEASE: '1' });
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-A.json')), 'first Cargo start');
    const second = launch(fixture, fixture.revisions[1], secondArgs, { FAKE_PRIOR_REVISION: fixture.revisions[0] });
    await waitFor(() => second.stderr.includes('Waiting for the shared build lock:'), 'second helper queue');
    const lock = spawnSync('flock', ['--nonblock', path.join(fixture.scratch, 'build.lock'), process.execPath, '-e', 'process.exit(0)'], { env: fixture.env });
    assert.equal(lock.status, 1, 'The real global flock must remain held while Cargo A waits');
    assert.equal(git(fixture, 'rev-parse', 'HEAD'), fixture.revisions[0]);
    assert.equal(fs.existsSync(path.join(fixture.directory, 'started-B.json')), false);
    fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue\n');
    const [firstOutcome, secondOutcome] = await Promise.all([first.done, second.done]);
    assert.equal(firstOutcome.code, 0, firstOutcome.stderr);
    assert.equal(secondOutcome.code, 0, secondOutcome.stderr);
    const firstReceipt = readReceipt(fixture, firstOutcome, fixture.revisions[0], firstArgs);
    const secondReceipt = readReceipt(fixture, secondOutcome, fixture.revisions[1], secondArgs);
    for (const { record } of [firstReceipt, secondReceipt]) {
      assert.equal(record.exit_code, 0);
      assert.deepEqual(record.receipt_errors, []);
    }
    assertExecutable(fixture, firstReceipt.record, 'binary:A\n');
    assertExecutable(fixture, secondReceipt.record, 'binary:B\n');
    assert.notEqual(firstReceipt.record.source_tree, secondReceipt.record.source_tree);
    assert.equal(fs.readFileSync(fixture.executable, 'utf8'), 'binary:B\n');
    const prior = JSON.parse(fs.readFileSync(path.join(fixture.directory, 'prior-receipt.json')));
    assert.equal(prior.name, path.basename(firstReceipt.filename));
    assert.equal(prior.body, firstReceipt.body.toString());
    assert.deepEqual(fs.readFileSync(firstReceipt.filename), firstReceipt.body);
    assert.equal(firstOutcome.stdout, 'Finished fake build\n');
    assert.equal(secondOutcome.stdout, 'Finished fake build\n');
    assert.doesNotMatch(secondOutcome.stderr, /\b\d+\s+(?:tests?\s+)?passed\b/i);
    assert.equal(git(fixture, 'rev-parse', 'HEAD'), fixture.revisions[1]);
  });
});

test('no executable event records an empty list despite a stale executable in the target', async () => {
  await withFixture(async fixture => {
    fs.mkdirSync(path.dirname(fixture.executable), { recursive: true });
    fs.writeFileSync(fixture.executable, 'stale executable\n', { mode: 0o700 });
    const args = ['build'];
    const outcome = await launch(fixture, fixture.revisions[0], args, { FAKE_CARGO_MODE: 'empty' }).done;
    assert.equal(outcome.code, 0, outcome.stderr);
    const { record } = readReceipt(fixture, outcome, fixture.revisions[0], args);
    assert.equal(record.exit_code, 0);
    assert.deepEqual(record.executables, []);
    assert.deepEqual(record.receipt_errors, []);
    assert.equal(fs.readFileSync(fixture.executable, 'utf8'), 'stale executable\n');
  });
});

test('Cargo failure still writes a receipt and preserves its nonzero exit code', async () => {
  await withFixture(async fixture => {
    const args = ['test', '--lib'];
    const outcome = await launch(fixture, fixture.revisions[0], args, { FAKE_CARGO_MODE: 'fail' }).done;
    assert.equal(outcome.code, 37, outcome.stderr);
    assert.match(outcome.stderr, /error: fake Cargo failure/);
    const { record } = readReceipt(fixture, outcome, fixture.revisions[0], args);
    assert.equal(record.exit_code, 37);
    assert.deepEqual(record.executables, []);
    assert.deepEqual(record.receipt_errors, []);
  });
});

test('a missing reported executable prevents a qualified successful receipt', async () => {
  await withFixture(async fixture => {
    const args = ['test', '--no-run'];
    const outcome = await launch(fixture, fixture.revisions[0], args, { FAKE_CARGO_MODE: 'missing' }).done;
    assert.notEqual(outcome.code, 0, outcome.stderr);
    const { record } = readReceipt(fixture, outcome, fixture.revisions[0], args);
    assert.equal(record.exit_code, 0, 'The receipt must retain the actual Cargo exit code');
    assert.deepEqual(record.executables, []);
    assert.ok(record.receipt_errors.length > 0);
    assert.ok(record.receipt_errors.some(error => error.includes(fixture.executable)));
    assert.equal(record.exit_code === 0 && record.signal === null && record.receipt_errors.length === 0, false);
  });
});

test('both target-dir spellings are refused before invoking Cargo', async () => {
  await withFixture(async fixture => {
    const override = path.join(fixture.directory, 'refused-target');
    for (const args of [['build', '--target-dir', override], ['test', '--target-dir=' + override]]) {
      const outcome = await launch(fixture, fixture.revisions[0], args).done;
      assert.notEqual(outcome.code, 0);
      assert.match(outcome.stderr, /target directory is centrally owned/);
      assert.doesNotMatch(outcome.stderr, /^Cargo receipt:/m);
      assert.equal(fs.existsSync(path.join(fixture.directory, 'started-A.json')), false);
      assert.equal(fs.existsSync(path.join(fixture.directory, 'started-B.json')), false);
      assert.equal(fs.existsSync(path.join(fixture.scratch, 'receipts')), false);
      assert.equal(fs.existsSync(path.join(fixture.scratch, 'build.lock.queue')), false);
      assert.equal(fs.existsSync(path.join(fixture.scratch, 'build.lock')), false);
      assert.equal(fs.existsSync(override), false);
      assert.equal(git(fixture, 'rev-parse', 'HEAD'), fixture.revisions[1]);
    }
  });
});

test('six future submissions enter in atomic registration order without holding the metadata lock', async () => {
  await withFixture(async fixture => {
    const submissions = [];
    for (let index = 0; index < 6; index++) {
      const child = launch(fixture, fixture.revisions[index % 2], ['build'], {
        FAKE_JOB_ID: String(index), ...(index === 0 ? { FAKE_WAIT_FOR_RELEASE: '1' } : {}),
      });
      submissions.push(child);
      await queued(fixture, child);
      if (index === 0) await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-0.json')), 'head Cargo start');
    }
    assert.deepEqual(tickets(fixture).map(ticket => ticket.pid), submissions.map(child => child.process.pid));
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock.queue/.lock')), 0);
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock')), 1);
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), '0\n');
    fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue');
    const outcomes = await Promise.all(submissions.map(child => child.done));
    outcomes.forEach(outcome => assert.equal(outcome.code, 0, outcome.stderr));
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), '0\n1\n2\n3\n4\n5\n');
    assert.deepEqual(tickets(fixture), []);
  });
});

test('SIGINT and SIGTERM cancel only the owned waiting ticket and preserve its successors', async () => {
  for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]]) {
    await withFixture(async fixture => {
      const first = launch(fixture, fixture.revisions[0], ['build'], { FAKE_WAIT_FOR_RELEASE: '1' });
      await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-A.json')), 'first Cargo start');
      const cancelled = launch(fixture, fixture.revisions[1], ['build'], { FAKE_JOB_ID: 'cancelled' });
      await queued(fixture, cancelled);
      const last = launch(fixture, fixture.revisions[1], ['build']);
      await queued(fixture, last);
      cancelled.process.kill(signal);
      const outcome = await cancelled.done;
      assert.equal(outcome.code, code, outcome.stderr);
      assert.doesNotMatch(outcome.stderr, /^Cargo receipt:/m);
      assert.deepEqual(tickets(fixture).map(ticket => ticket.pid), [first.process.pid, last.process.pid]);
      fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue');
      for (const child of [first, last]) {
        const result = await child.done;
        assert.equal(result.code, 0, result.stderr);
      }
      assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), 'A\nB\n');
      assert.deepEqual(tickets(fixture), []);
    });
  }
});

test('a cancelled queue head leaves an existing legacy flock owner and future FIFO order intact', async () => {
  await withFixture(async fixture => {
    const legacy = legacyHolder(fixture);
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'legacy-started')), 'legacy lock ownership');
    const head = launch(fixture, fixture.revisions[0], ['build'], { FAKE_JOB_ID: 'cancelled' });
    await queued(fixture, head);
    const second = launch(fixture, fixture.revisions[0], ['build']);
    await queued(fixture, second);
    const third = launch(fixture, fixture.revisions[1], ['build']);
    await queued(fixture, third);
    head.process.kill('SIGTERM');
    assert.equal((await head.done).code, 143);
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock')), 1);
    assert.equal(fs.existsSync(path.join(fixture.directory, 'order')), false);
    fs.writeFileSync(path.join(fixture.directory, 'legacy-release'), 'continue');
    assert.equal((await legacy.done).code, 0);
    for (const child of [second, third]) {
      const outcome = await child.done;
      assert.equal(outcome.code, 0, outcome.stderr);
    }
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), 'A\nB\n');
    assert.deepEqual(tickets(fixture), []);
  });
});

test('an unlocked stale ticket with a reused live PID cannot block a future submission', async () => {
  await withFixture(async fixture => {
    const directory = path.join(fixture.scratch, 'build.lock.queue');
    fs.mkdirSync(directory);
    const token = '00000000-0000-4000-8000-000000000000';
    const stale = path.join(directory, '00000000000000000001-' + token + '.ticket');
    const pending = path.join(directory, 'pending-' + token);
    for (const filename of [stale, pending]) fs.writeFileSync(filename, JSON.stringify({ pid: process.pid, token }));
    const outcome = await launch(fixture, fixture.revisions[0], ['build']).done;
    assert.equal(outcome.code, 0, outcome.stderr);
    assert.equal(fs.existsSync(stale), false);
    assert.equal(fs.existsSync(pending), false);
    assert.deepEqual(tickets(fixture), []);
  });
});

test('a killed queued owner is cleared when its predecessor releases without a new registration', async () => {
  await withFixture(async fixture => {
    const first = launch(fixture, fixture.revisions[0], ['build'], { FAKE_WAIT_FOR_RELEASE: '1' });
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-A.json')), 'first Cargo start');
    const dead = launch(fixture, fixture.revisions[1], ['build'], { FAKE_JOB_ID: 'dead' });
    await queued(fixture, dead);
    const last = launch(fixture, fixture.revisions[1], ['build']);
    await queued(fixture, last);
    dead.process.kill('SIGKILL');
    assert.equal((await dead.done).signal, 'SIGKILL');
    assert.ok(tickets(fixture).some(ticket => ticket.pid === dead.process.pid));
    fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue');
    for (const child of [first, last]) {
      const outcome = await child.done;
      assert.equal(outcome.code, 0, outcome.stderr);
    }
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), 'A\nB\n');
    assert.deepEqual(tickets(fixture), []);
  });
});

test('active cancellation retains queue and heavy ownership until Cargo closes and its receipt is bound', async () => {
  await withFixture(async fixture => {
    const args = ['build'];
    const active = launch(fixture, fixture.revisions[0], args, { FAKE_WAIT_FOR_RELEASE: '1', FAKE_DELAY_SIGNAL: '1' });
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-A.json')), 'active Cargo start');
    const next = launch(fixture, fixture.revisions[1], args, { FAKE_PRIOR_REVISION: fixture.revisions[0] });
    await queued(fixture, next);
    active.process.kill('SIGTERM');
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'signal-A')), 'Cargo signal acknowledgement');
    const head = tickets(fixture)[0];
    assert.equal(head.pid, active.process.pid);
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock.queue', head.name)), 1);
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock')), 1);
    assert.equal(fs.existsSync(path.join(fixture.directory, 'started-B.json')), false);
    fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue');
    const firstOutcome = await active.done;
    const nextOutcome = await next.done;
    assert.equal(firstOutcome.code, 143, firstOutcome.stderr);
    assert.equal(nextOutcome.code, 0, nextOutcome.stderr);
    const { record } = readReceipt(fixture, firstOutcome, fixture.revisions[0], args, 'SIGTERM');
    assert.equal(record.exit_code, 143);
    assert.deepEqual(record.receipt_errors, []);
    assertExecutable(fixture, record, 'binary:A\n');
    assert.deepEqual(tickets(fixture), []);
  });
});

test('a killed active submitter retains its inherited child ticket until the locked child closes', async () => {
  await withFixture(async fixture => {
    const active = launch(fixture, fixture.revisions[0], ['build'], { FAKE_WAIT_FOR_RELEASE: '1' });
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-A.json')), 'active Cargo start');
    const exited = new Promise(resolve => active.process.once('exit', (code, signal) => resolve({ code, signal })));
    active.process.kill('SIGKILL');
    assert.equal((await exited).signal, 'SIGKILL');
    const next = launch(fixture, fixture.revisions[1], ['build'], { FAKE_PRIOR_REVISION: fixture.revisions[0] });
    await queued(fixture, next);
    assert.deepEqual(tickets(fixture).map(ticket => ticket.pid), [active.process.pid, next.process.pid]);
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock')), 1);
    assert.equal(fs.existsSync(path.join(fixture.directory, 'started-B.json')), false);
    fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue');
    const activeOutcome = await active.done;
    const nextOutcome = await next.done;
    assert.equal(activeOutcome.signal, 'SIGKILL');
    assert.equal(nextOutcome.code, 0, nextOutcome.stderr);
    readReceipt(fixture, activeOutcome, fixture.revisions[0], ['build']);
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), 'A\nB\n');
    assert.deepEqual(tickets(fixture), []);
  });
});

test('Suite can queue one outer flock around --inside no-run and its Node adapter without nesting the heavy lock', async () => {
  await withFixture(async fixture => {
    const wrapper = path.join(fixture.directory, 'suite.mjs');
    const inside = path.join(fixture.directory, 'suite-inside.mjs');
    fs.writeFileSync(wrapper, `
      import fs from 'node:fs';
      import { spawn } from 'node:child_process';
      import { withBuildQueue } from ${JSON.stringify(new URL('./build-queue.mjs', import.meta.url).href)};
      const config = JSON.parse(fs.readFileSync(process.env.FIXTURE_CONFIG));
      await withBuildQueue(config.lock, async ({ ticketFd }) => {
        const child = spawn('flock', ['--no-fork', config.lock, process.execPath, ${JSON.stringify(inside)}], {
          stdio: ['inherit', 'inherit', 'inherit', ticketFd],
        });
        process.exitCode = await new Promise((resolve, reject) => {
          child.once('error', reject);
          child.once('close', code => resolve(code ?? 1));
        });
      });
    `);
    fs.writeFileSync(inside, `
      import fs from 'node:fs';
      import path from 'node:path';
      import { spawnSync } from 'node:child_process';
      import { setTimeout as delay } from 'node:timers/promises';
      const state = process.env.FIXTURE_STATE;
      const config = JSON.parse(fs.readFileSync(process.env.FIXTURE_CONFIG));
      const result = spawnSync(process.execPath, [${JSON.stringify(helper)}, '--inside', process.env.FIXTURE_CONFIG, 'fixture', process.env.FIXTURE_REVISION, 'test', '--no-run'], { stdio: 'inherit' });
      if (result.status !== 0) throw new Error('Inside helper failed');
      const probe = spawnSync('flock', ['--nonblock', config.lock, process.execPath, '-e', 'process.exit(0)']);
      if (probe.status !== 1) throw new Error('Adapter lost the outer heavy lock');
      fs.writeFileSync(path.join(state, 'adapter-started'), 'under the same lock');
      while (!fs.existsSync(path.join(state, 'adapter-release'))) await delay(10);
    `);
    const first = launch(fixture, fixture.revisions[0], ['build'], { FAKE_WAIT_FOR_RELEASE: '1' });
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'started-A.json')), 'first Cargo start');
    const suite = launchOwned(fixture, process.execPath, [wrapper], {
      FIXTURE_CONFIG: fixture.config, FIXTURE_REVISION: fixture.revisions[1], FAKE_JOB_ID: 'suite',
    });
    await queued(fixture, suite);
    const last = launch(fixture, fixture.revisions[0], ['build'], { FAKE_JOB_ID: 'last' });
    await queued(fixture, last);
    fs.writeFileSync(path.join(fixture.directory, 'release'), 'continue');
    await waitFor(() => fs.existsSync(path.join(fixture.directory, 'adapter-started')), 'locked Suite adapter');
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), 'A\nsuite\n');
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock')), 1);
    assert.equal(probeLock(fixture, path.join(fixture.scratch, 'build.lock.queue/.lock')), 0);
    fs.writeFileSync(path.join(fixture.directory, 'adapter-release'), 'continue');
    const suiteOutcome = await suite.done;
    assert.equal(suiteOutcome.code, 0, suiteOutcome.stderr);
    const receipt = readReceipt(fixture, suiteOutcome, fixture.revisions[1], ['test', '--no-run']);
    assert.equal(suiteOutcome.stdout, 'Finished fake build\n');
    assert.equal(receipt.record.exit_code, 0);
    for (const child of [first, last]) {
      const outcome = await child.done;
      assert.equal(outcome.code, 0, outcome.stderr);
    }
    assert.equal(fs.readFileSync(path.join(fixture.directory, 'order'), 'utf8'), 'A\nsuite\nlast\n');
    assert.deepEqual(tickets(fixture), []);
  });
});

test('malformed revisions, commands, repositories and receipt format overrides are rejected before queue work', async () => {
  await withFixture(async fixture => {
    const invocations = [
      [fixture.config, 'fixture', 'bad-revision', 'build'],
      [fixture.config, 'fixture', fixture.revisions[0], 'unknown-command'],
      [fixture.config, 'unknown-repository', fixture.revisions[0], 'build'],
      [fixture.config, 'fixture', fixture.revisions[0], 'build', '--message-format', 'json'],
      [fixture.config, 'fixture', fixture.revisions[0], 'test', '--message-format=json'],
    ];
    for (const invocation of invocations) {
      const outcome = await launchOwned(fixture, process.execPath, [helper, ...invocation]).done;
      assert.notEqual(outcome.code, 0);
      assert.doesNotMatch(outcome.stderr, /^Cargo receipt:/m);
      assert.equal(fs.existsSync(path.join(fixture.scratch, 'build.lock.queue')), false);
      assert.equal(fs.existsSync(path.join(fixture.scratch, 'build.lock')), false);
      assert.equal(fs.existsSync(path.join(fixture.directory, 'order')), false);
      assert.deepEqual(fs.readdirSync(fixture.scratch), []);
      assert.equal(git(fixture, 'rev-parse', 'HEAD'), fixture.revisions[1]);
    }
  });
});
