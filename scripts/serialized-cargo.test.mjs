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
    fs.writeFileSync(path.join(state, 'started-' + marker + '.json'), JSON.stringify({
      args, revision, marker, executable,
      target: process.env.CARGO_TARGET_DIR,
      jobs: process.env.CARGO_BUILD_JOBS,
      temporary: [process.env.TMPDIR, process.env.TMP, process.env.TEMP],
    }));
    if (process.env.FAKE_WAIT_FOR_RELEASE === '1') {
      const deadline = Date.now() + 8000;
      while (!fs.existsSync(path.join(state, 'release'))) {
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
  const child = spawn(process.execPath, [helper, fixture.config, 'fixture', revision, ...args], {
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
function readReceipt(fixture, outcome, revision, args) {
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
    'started_at', 'finished_at', 'helper_sha256', 'receipt_helper_sha256', 'config_sha256',
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
  assert.equal(record.signal, null);
  assert.equal(record.helper_sha256, sha256(fs.readFileSync(helper)));
  assert.equal(record.receipt_helper_sha256, sha256(fs.readFileSync(receiptHelper)));
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
      assert.equal(fs.existsSync(override), false);
      assert.equal(git(fixture, 'rev-parse', 'HEAD'), fixture.revisions[1]);
    }
  });
});
