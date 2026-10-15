#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceBudget = 512 * 1024 * 1024;
const temporaryBudget = 1024 * 1024 * 1024;
const digestPattern = /^[a-f0-9]{64}$/;
const namePattern = /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/;
const filter = 'formats::raw::qualification::raw_camera_corpus';
const states = new Set(['not_yet_qualified', 'fixture_unavailable', 'needs_resource_scheduling']);

/** Check an object contract. Args: object, allowed fields and label. Returns nothing or throws. */
function fields(value, names, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some(key => !names.includes(key)) || names.some(key => !(key in value))) throw new Error(`Invalid ${label} fields`);
}

/** Check a normalized tolerance. Args: bounds. Returns whether finite ordered values lie in 0..1. */
function tolerance(value) {
  return value && Object.keys(value).sort().join(',') === 'max,mean' && Number.isFinite(value.mean) && Number.isFinite(value.max) && value.mean >= 0 && value.max >= value.mean && value.max <= 1;
}

/** Read the actual advertised inventory. Args: raw.rs text. Returns its declared extension list. */
export function extensions(text) {
  const declaration = text.match(/pub const EXTENSIONS:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/);
  if (!declaration) throw new Error('RAW extension declaration not found');
  return [...declaration[1].matchAll(/"([a-z0-9]+)"/g)].map(match => match[1]);
}

/** Validate metadata without filling evidence gaps. Args: manifest and advertised extensions. Returns the manifest or throws. */
export function validateManifest(manifest, advertised) {
  fields(manifest, ['schema_version', 'scope', 'provenance', 'budgets', 'families', 'cases'], 'manifest');
  fields(manifest.budgets, ['source_bytes', 'temporary_bytes', 'parallel_cases'], 'budgets');
  if (manifest.schema_version !== 1 || !manifest.scope || !manifest.provenance || manifest.budgets.source_bytes !== sourceBudget || manifest.budgets.temporary_bytes !== temporaryBudget || manifest.budgets.parallel_cases !== 1) throw new Error('Unsupported manifest version/scope/acquisition budget');
  if (!Array.isArray(manifest.families) || !Array.isArray(manifest.cases) || !manifest.cases.length) throw new Error('Zero camera cases');
  const listed = manifest.families.map(family => family.extension);
  if (new Set(listed).size !== listed.length || [...listed].sort().join(',') !== [...advertised].sort().join(',')) throw new Error('Manifest must inventory every advertised extension exactly once');
  const ids = new Set();
  for (const entry of manifest.cases) {
    fields(entry, ['id', 'extension', 'required', 'status', 'identity', 'source', 'required_reference_kinds', 'references', 'preview_tolerance', 'notes'], 'camera case');
    fields(entry.source, ['filename', 'url', 'sha256', 'size_bytes', 'license', 'provenance'], 'source');
    fields(entry.identity, ['make', 'model', 'firmware', 'compression', 'cfa', 'bit_depth', 'sensor_dimensions', 'active_dimensions', 'output_dimensions', 'orientation', 'camera_wb', 'profile', 'crop', 'opcodes', 'frames'], 'identity');
    if (!namePattern.test(entry.id) || ids.has(entry.id) || !advertised.includes(entry.extension) || typeof entry.required !== 'boolean' || !states.has(entry.status) || !namePattern.test(entry.source.filename) || path.extname(entry.source.filename) !== `.${entry.extension}` || !tolerance(entry.preview_tolerance)) throw new Error(`Invalid camera case ${entry.id}`);
    ids.add(entry.id);
    for (const key of ['make', 'model', 'firmware', 'compression', 'cfa', 'profile', 'opcodes']) if (entry.identity[key] !== null && (typeof entry.identity[key] !== 'string' || !entry.identity[key])) throw new Error(`Invalid ${key}`);
    for (const key of ['sensor_dimensions', 'active_dimensions', 'output_dimensions']) if (entry.identity[key] !== null && (!Array.isArray(entry.identity[key]) || entry.identity[key].length !== 2 || !entry.identity[key].every(n => Number.isInteger(n) && n > 0 && n <= 0xffffffff))) throw new Error('Invalid geometry');
    if (entry.identity.bit_depth !== null && (!Number.isInteger(entry.identity.bit_depth) || entry.identity.bit_depth <= 0 || entry.identity.bit_depth > 32)) throw new Error('Invalid sensor depth');
    if (entry.identity.orientation !== null && (!Number.isInteger(entry.identity.orientation) || entry.identity.orientation < 0 || entry.identity.orientation > 15)) throw new Error('Invalid orientation');
    if (entry.identity.camera_wb !== null && (!Array.isArray(entry.identity.camera_wb) || entry.identity.camera_wb.length !== 4 || !entry.identity.camera_wb.every(n => Number.isFinite(n) && n >= 0))) throw new Error('Invalid white balance');
    if (entry.identity.crop !== null && (!Array.isArray(entry.identity.crop) || entry.identity.crop.length !== 4 || !entry.identity.crop.every(n => Number.isInteger(n) && n >= 0))) throw new Error('Invalid crop');
    if (entry.identity.frames !== null && (!Number.isInteger(entry.identity.frames) || entry.identity.frames <= 0)) throw new Error('Invalid frame count');
    if (entry.source.sha256 !== null && !digestPattern.test(entry.source.sha256)) throw new Error('Invalid source SHA-256');
    if (entry.source.size_bytes !== null && (!Number.isSafeInteger(entry.source.size_bytes) || entry.source.size_bytes <= 0)) throw new Error('Invalid source size');
    if (!Array.isArray(entry.references) || !Array.isArray(entry.required_reference_kinds) || new Set(entry.required_reference_kinds).size !== entry.required_reference_kinds.length || !entry.required_reference_kinds.includes('libraw_integration') || !entry.required_reference_kinds.some(kind => ['independent_render', 'independent_sensor'].includes(kind)) || entry.required_reference_kinds.includes('display_look')) throw new Error('Missing required integration/independent reference kinds');
    const referenceIds = new Set();
    for (const reference of entry.references) {
      fields(reference, ['id', 'kind', 'representation', 'filename', 'sha256', 'dimensions', 'sensor_fnv1a64', 'toolchain', 'provenance', 'tolerance'], 'reference');
      fields(reference.toolchain, ['name', 'version', 'executable_sha256', 'archive_sha256', 'platform', 'command'], 'reference toolchain');
      if (!namePattern.test(reference.id) || referenceIds.has(reference.id) || !['libraw_integration', 'independent_render', 'independent_sensor', 'display_look'].includes(reference.kind) || !['oriented-linear-srgb-rgb16le', 'display-srgb-rgba16le', 'sensor-u16le-masked'].includes(reference.representation) || (reference.kind === 'independent_sensor') !== (reference.representation === 'sensor-u16le-masked') || !tolerance(reference.tolerance) || !reference.provenance || !reference.toolchain.name || !Array.isArray(reference.toolchain.command) || !reference.toolchain.command.every(s => typeof s === 'string')) throw new Error('Invalid reference contract');
      referenceIds.add(reference.id);
      if (reference.filename !== null && !namePattern.test(reference.filename)) throw new Error('Unsafe reference filename');
      for (const hash of [reference.sha256, reference.toolchain.executable_sha256, reference.toolchain.archive_sha256]) if (hash !== null && !digestPattern.test(hash)) throw new Error('Invalid reference/tool SHA-256');
      if (reference.sensor_fnv1a64 !== null && !/^[a-f0-9]{16}$/.test(reference.sensor_fnv1a64)) throw new Error('Invalid sensor FNV identity');
      if (reference.dimensions !== null && (!Array.isArray(reference.dimensions) || reference.dimensions.length !== 2 || !reference.dimensions.every(n => Number.isInteger(n) && n > 0))) throw new Error('Invalid reference geometry');
    }
    if (entry.required_reference_kinds.some(kind => !entry.references.some(reference => reference.kind === kind))) throw new Error('Missing declared reference kind');
  }
  for (const family of manifest.families) {
    fields(family, ['extension', 'family', 'status', 'mode_axes', 'candidate_cases', 'unresolved_modes', 'evidence'], 'family');
    if (!family.family || !states.has(family.status) || !family.mode_axes?.length || !family.unresolved_modes?.length || !family.evidence?.length || !family.candidate_cases?.length || family.candidate_cases.some(id => !manifest.cases.some(entry => entry.id === id && entry.extension === family.extension))) throw new Error(`Incomplete family inventory ${family.extension}`);
  }
  if (manifest.cases.some(entry => !manifest.families.some(family => family.candidate_cases.includes(entry.id)))) throw new Error('Case absent from family inventory');
  return manifest;
}

/** Count explicit outcomes and reject empty required evidence. Args: reports and mode. Returns aggregate counts and pass/fail. */
export function aggregate(reports, mode) {
  const counts = { selected: reports.length, checked: reports.filter(r => r.checked === true).length, skipped: reports.filter(r => r.status === 'skipped').length, failed: reports.filter(r => r.status === 'failed').length, passed: reports.filter(r => r.status === 'passed').length };
  const outcomesValid = reports.every(report => ['passed', 'failed', 'skipped'].includes(report.status) && (report.status !== 'passed' || report.checked === true));
  return { ...counts, success: ['required', 'optional'].includes(mode) && outcomesValid && counts.selected > 0 && counts.failed === 0 && (mode === 'optional' || (counts.checked === counts.selected && counts.skipped === 0 && counts.passed === counts.selected)) };
}

/** Stream a regular file digest. Args: path. Returns its SHA-256 without loading the source in memory. */
async function hashFile(filename) {
  if (!fs.statSync(filename).isFile()) throw new Error('Expected a regular file');
  const hash = crypto.createHash('sha256');
  for await (const chunk of fs.createReadStream(filename)) hash.update(chunk);
  return hash.digest('hex');
}

/** Check a supplied file pin. Args: path and expected SHA-256. Returns an observed digest plus any classified error. */
async function inspectFile(filename, expected) {
  let actual = null;
  try { if (filename) actual = await hashFile(filename); } catch (error) { return { actual, error: { kind: 'missing', detail: `${filename}: ${error.message}` } }; }
  if (!expected || !filename || actual === null) return { actual, error: { kind: 'missing', detail: 'Missing path or pinned SHA-256' } };
  if (actual !== expected) return { actual, error: { kind: 'invalid', detail: `${filename}: expected ${expected}, observed ${actual}` } };
  return { actual };
}

/** Read exact case path bindings. Args: optional bindings JSON path and manifest. Returns a validated binding map. */
function readBindings(filename, manifest) {
  if (!filename) return {};
  const bindings = JSON.parse(fs.readFileSync(filename, 'utf8'));
  fields(bindings, ['schema_version', 'cases'], 'bindings');
  if (bindings.schema_version !== 1 || !bindings.cases || typeof bindings.cases !== 'object' || Array.isArray(bindings.cases)) throw new Error('Invalid bindings version');
  for (const [id, binding] of Object.entries(bindings.cases)) {
    const entry = manifest.cases.find(item => item.id === id);
    if (!entry || !binding || Object.keys(binding).some(key => !['source', 'references', 'executables'].includes(key))) throw new Error(`Unknown case/binding ${id}`);
    for (const key of ['references', 'executables']) {
      if (binding[key] && (typeof binding[key] !== 'object' || Array.isArray(binding[key]) || Object.keys(binding[key]).some(ref => !entry.references.some(r => r.id === ref)))) throw new Error('Unknown reference binding');
    }
    for (const file of [binding.source, ...Object.values(binding.references ?? {}), ...Object.values(binding.executables ?? {})].filter(Boolean)) if (typeof file !== 'string' || !path.isAbsolute(file)) throw new Error('Binding paths must be absolute');
  }
  return bindings.cases;
}

/** Inspect one case before any process starts. Args: case, bindings and corpus root. Returns source and reference receipts. */
export async function inspectCase(entry, binding, corpus) {
  const source = binding.source ?? path.join(corpus, 'raw', entry.source.filename);
  const checked = await inspectFile(source, entry.source.sha256);
  const errors = checked.error ? [checked.error] : [];
  for (const key of ['make', 'model', 'compression', 'cfa', 'bit_depth', 'sensor_dimensions', 'active_dimensions', 'output_dimensions', 'orientation', 'camera_wb', 'profile']) {
    if (entry.identity[key] === null || entry.identity[key] === '') errors.push({kind:'missing', detail:`Unpinned camera identity: ${key}`});
  }
  if (!entry.source.url || !entry.source.license || !entry.source.provenance) errors.push({kind:'missing', detail:'Source URL/license/provenance is incomplete'});
  const bytes = checked.actual === null ? null : fs.statSync(source).size;
  if (bytes > sourceBudget) errors.push({ kind: 'scheduling_required', detail: 'Source exceeds the 512 MiB acquisition/test budget; parent resource scheduling required. No product support conclusion.' });
  if (bytes !== null && entry.source.size_bytes !== null && bytes !== entry.source.size_bytes) errors.push({ kind: 'invalid', detail: 'Source length differs from pin' });
  const references = [];
  for (const reference of entry.references.filter(ref => ref.kind !== 'display_look')) {
    if (!reference.dimensions || !reference.toolchain.version || !reference.toolchain.platform || !reference.toolchain.command.length) errors.push({kind:'missing', detail:`${reference.id}: missing reference geometry/toolchain version/platform/recipe`});
    const referencePath = binding.references?.[reference.id] ?? (reference.filename ? path.join(corpus, 'references', reference.filename) : null);
    const observed = await inspectFile(referencePath, reference.sha256);
    const executable = await inspectFile(binding.executables?.[reference.id], reference.toolchain.executable_sha256);
    for (const error of [observed.error, executable.error].filter(Boolean)) errors.push({ ...error, detail: `${reference.id}: ${error.detail}` });
    references.push({ id: reference.id, kind: reference.kind, representation: reference.representation, source_sha256: entry.source.sha256, reference_sha256: observed.actual, executable_sha256: executable.actual, toolchain: reference.toolchain });
  }
  return { source, bytes, source_before_sha256: checked.actual, references, errors, processable: !checked.error && bytes <= sourceBudget && !errors.some(e => e.kind === 'invalid' && e.detail === 'Source length differs from pin') };
}

/** Read the shared heavy lock configuration. Args: config file. Returns a validated build config. */
function buildConfig(filename) {
  const config = JSON.parse(fs.readFileSync(filename, 'utf8'));
  if (!config.scratch || !config.lock || !config.repositories?.omadesign || !path.isAbsolute(config.lock) || !path.resolve(config.lock).startsWith(`${path.resolve(config.scratch)}/`) || !path.resolve(config.scratch).startsWith('/home/')) throw new Error('Use the existing serialized-cargo global heavy-lock configuration');
  return config;
}

/** Plan or execute a strictly serial corpus run. Args: command options. Returns an aggregate exit status. */
async function qualify(options, command) {
  const manifestPath = path.resolve(options.manifest ?? path.join(root, 'docs/qa/raw-camera-manifest.json'));
  const manifest = validateManifest(JSON.parse(fs.readFileSync(manifestPath, 'utf8')), extensions(fs.readFileSync(path.join(root, 'src/formats/raw.rs'), 'utf8')));
  const mode = options.mode ?? 'required';
  if (!['required', 'optional'].includes(mode)) throw new Error('Mode must be required or optional');
  const cases = options.case ? manifest.cases.filter(entry => entry.id === options.case) : manifest.cases;
  if (!cases.length) throw new Error('Zero camera cases selected');
  const bindings = readBindings(options.bindings, manifest);
  const corpus = path.resolve(options.corpus ?? path.join(root, '.qa-runs/raw-corpus'));
  let output;
  if (command === 'run') {
    if (!options.config || !/^[a-f0-9]{40}$/.test(options.revision ?? '') || !digestPattern.test(options['runner-sha256'] ?? '') || !options.output) throw new Error('Run requires --config, --revision FULL_COMMIT, --runner-sha256 and --output NEW_DIRECTORY');
    buildConfig(options.config);
    output = path.resolve(options.output);
    if (!output.startsWith(`${root}/.qa-runs/`) || fs.realpathSync(path.dirname(output)) !== path.dirname(output)) throw new Error('Output must have a canonical existing parent inside this checkout .qa-runs');
    fs.mkdirSync(output);
  }
  const reports = [];
  for (const entry of cases) {
    const binding = bindings[entry.id] ?? {};
    const preparation = await inspectCase(entry, binding, corpus);
    let report = { schema_version: 1, case: entry.id, required: entry.required, checked: false, status: command === 'plan' ? 'planned' : mode === 'optional' && !preparation.errors.some(e => e.kind === 'invalid') ? 'skipped' : 'failed', source_before_sha256: preparation.source_before_sha256, source_after_sha256: preparation.source_before_sha256, required_reference_kinds: entry.required_reference_kinds, references: preparation.references, comparisons: [], errors: preparation.errors, acquisition: { url: entry.source.url, license: entry.source.license, pinned_sha256: entry.source.sha256, bytes: preparation.bytes, serial: true } };
    if (command === 'run' && preparation.processable) {
      const nativeOutput = path.join(output, `${entry.id}-native`);
      const args = ['/home/michael/Projects/omadesign/.artifacts/craft-gap-stack/tooling/c4a4ce84171cd41c0db544efbb1c99c57e4bd26c/scripts/serialized-cargo.mjs', path.resolve(options.config), 'omadesign', options.revision, 'test', '--locked', '--lib', '--', filter, '--ignored', '--exact', '--nocapture', '--test-threads=1'];
      const result = spawnSync(process.execPath, args, { cwd: root, encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, env: { ...process.env, OMA_RAW_MANIFEST: manifestPath, OMA_RAW_MODE: mode, OMA_RAW_CORPUS: corpus, OMA_RAW_OUTPUT: nativeOutput, OMA_RAW_CASE: entry.id, OMA_RAW_CASE_BINDINGS: JSON.stringify(binding), OMA_RAW_RUNNER_SHA256: options['runner-sha256'], OMA_RAW_REVISION: options.revision } });
      fs.writeFileSync(path.join(output, `${entry.id}.log`), `${result.stdout ?? ''}${result.stderr ?? ''}`, { flag: 'wx' });
      const nativeReceipt = path.join(nativeOutput, entry.id, 'result.json');
      if (fs.existsSync(nativeReceipt)) {
        try {
          const native = JSON.parse(fs.readFileSync(nativeReceipt, 'utf8'));
          if (native.schema_version !== 1 || native.case !== entry.id || native.runner?.sha256 !== options['runner-sha256'] || native.runner?.revision !== options.revision || !['passed', 'failed', 'skipped'].includes(native.status) || !Array.isArray(native.comparisons) || !Array.isArray(native.errors) || native.source_before_sha256 !== preparation.source_before_sha256) throw new Error('Invalid native runner receipt');
          report = native;
          for (const comparison of report.comparisons) {
            const reference = entry.references.find(item => item.id === comparison.reference);
            const stats = comparison.stats;
            if (!reference || comparison.kind !== reference.kind || comparison.representation !== reference.representation || comparison.reference_sha256 !== reference.sha256 || !stats || !Number.isSafeInteger(stats.samples) || stats.samples <= 0 || !Number.isFinite(stats.mean) || !Number.isFinite(stats.max) || stats.mean < 0 || stats.mean > stats.max || stats.max > 1 || !Array.isArray(stats.channel_mean) || !Array.isArray(stats.channel_max) || stats.channel_mean.length !== 3 || stats.channel_max.length !== 3 || !stats.channel_mean.every(Number.isFinite) || !stats.channel_max.every(Number.isFinite) || (comparison.passed === true && (stats.mean > reference.tolerance.mean || stats.max > reference.tolerance.max))) throw new Error('Invalid/zero reference comparison receipt');
          }
          for (const issue of preparation.errors) if (!report.errors.some(error => error.detail === issue.detail)) report.errors.push(issue);
        } catch (error) {
          report.status = 'failed';
          report.errors.push({ kind: 'runner', detail: error.message });
        }
      } else report.errors.push({ kind: 'runner', detail: result.error?.message ?? `Native receipt absent; helper exit ${result.status}, signal ${result.signal}` });
      if (result.status !== 0 || result.error) { report.status = 'failed'; report.errors.push({ kind: 'runner', detail: result.error?.message ?? `Serialized helper exited ${result.status}, signal ${result.signal}` }); }
    }
    report.source_after_sha256 = (await inspectFile(preparation.source, entry.source.sha256)).actual;
    if (command === 'run') {
      const actualKinds = new Set(report.comparisons.filter(item => item.passed === true && item.stats?.samples > 0).map(item => item.kind));
      if (report.status === 'passed' && (report.checked !== true || report.errors.length || entry.required_reference_kinds.some(kind => !actualKinds.has(kind)))) { report.status = 'failed'; report.errors.push({ kind: 'runner', detail: 'Claimed pass without every required nonzero reference comparison' }); }
      if (report.source_before_sha256 !== report.source_after_sha256) { report.status = 'failed'; report.errors.push({ kind: 'invalid', detail: 'Original source changed' }); }
      if (mode === 'required' && report.status === 'skipped') report.status = 'failed';
      fs.writeFileSync(path.join(output, `${entry.id}.json`), JSON.stringify(report) + '\n', { flag: 'wx' });
    }
    console.log(JSON.stringify(report));
    reports.push(report);
  }
  const summary = { schema_version: 1, command, mode, ...aggregate(reports, mode), manifest_sha256: await hashFile(manifestPath), qualification_executed: command === 'run' };
  if (output) fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(summary) + '\n', { flag: 'wx' });
  console.log(JSON.stringify(summary));
  return command === 'plan' || summary.success ? 0 : 1;
}

/** Exercise strict metadata contracts without camera decoding. Args: none. Returns zero after assertions. */
async function selfTest() {
  const text = fs.readFileSync(path.join(root, 'docs/qa/raw-camera-manifest.json'), 'utf8');
  const advertised = extensions(fs.readFileSync(path.join(root, 'src/formats/raw.rs'), 'utf8'));
  const manifest = validateManifest(JSON.parse(text), advertised);
  const mutations = [
    value => { value.schema_version = 2; },
    value => { value.cases = []; },
    value => { value.families.pop(); },
    value => { value.cases[0].future_required_feature = true; },
    value => { value.cases[0].source.sha256 = 'not-a-hash'; },
    value => { value.cases[0].source.filename = '../original.raw'; },
    value => { value.cases[0].identity.output_dimensions = [0, 48]; },
    value => { value.cases[0].required_reference_kinds = ['libraw_integration']; },
    value => { value.cases[0].references[0].representation = 'sensor-u16le-masked'; },
    value => { value.cases[0].references[0].toolchain.executable_sha256 = 'not-an-executable-pin'; },
  ];
  for (const mutate of mutations) { const value = JSON.parse(text); mutate(value); assert.throws(() => validateManifest(value, advertised)); }
  assert.equal(aggregate([], 'required').success, false);
  assert.equal(aggregate([{ checked: false, status: 'skipped' }], 'required').success, false);
  assert.equal(aggregate([{ checked: false, status: 'skipped' }], 'optional').success, true);
  assert.equal(aggregate([{ checked: true, status: 'failed' }], 'optional').success, false);
  assert.equal(aggregate([{ checked: true, status: 'passed' }, { checked: false, status: 'passed' }], 'required').success, false);
  assert.equal(aggregate([{ checked: false, status: 'passed' }], 'optional').success, false);
  assert.equal(aggregate([{ checked: false, status: 'planned' }], 'optional').success, false);
  const scratchRoot = path.join(root, '.qa-runs');
  fs.mkdirSync(scratchRoot, { recursive: true });
  const scratch = fs.mkdtempSync(path.join(scratchRoot, 'raw-node-contract-'));
  const original = path.join(scratch, 'own-bytes.raw');
  try {
    fs.writeFileSync(original, Buffer.from([11, 23, 47, 59]), { flag: 'wx' });
    const hash = await hashFile(original);
    assert.equal((await inspectFile(original, hash)).error, undefined);
    assert.equal((await inspectFile(original, null)).error.kind, 'missing');
    assert.equal((await inspectFile(original, '0'.repeat(64))).error.kind, 'invalid');
    assert.equal((await inspectFile(path.join(scratch, 'absent.raw'), hash)).error.kind, 'missing');
    const missing = await inspectCase(manifest.cases[0], {}, scratch);
    assert.equal(missing.processable, false);
    assert(missing.errors.some(error => error.kind === 'missing'));
    const badReference = structuredClone(manifest.cases[0]);
    badReference.source.sha256 = hash;
    badReference.references[0].sha256 = '0'.repeat(64);
    const inspected = await inspectCase(badReference, { source: original, references: { [badReference.references[0].id]: original } }, scratch);
    assert(inspected.errors.some(error => error.kind === 'invalid'));
    assert.deepEqual(fs.readFileSync(original), Buffer.from([11, 23, 47, 59]));
  } finally {
    fs.unlinkSync(original);
    fs.rmdirSync(scratch);
  }
  console.log(JSON.stringify({ command: 'self-test', status: 'passed', contract_mutations: mutations.length, camera_decodes: 0 }));
  return 0;
}

/** Acquire a pinned source while holding the shared lock. Args: options. Returns a serial acquisition exit status. */
async function acquire(options) {
  if (!options.config || !options.case || !options.corpus) throw new Error('Acquire requires --config, --case and --corpus');
  const config = buildConfig(options.config);
  if (!options['lock-held']) {
    const result = spawnSync('flock', [config.lock, process.execPath, fileURLToPath(import.meta.url), ...process.argv.slice(2), '--lock-held'], { stdio: 'inherit', env: { ...process.env, OMA_RAW_ACQUISITION_LOCK: path.resolve(config.lock) } });
    if (result.error) throw result.error;
    return result.status ?? 1;
  }
  if (process.env.OMA_RAW_ACQUISITION_LOCK !== path.resolve(config.lock)) throw new Error('Internal acquisition lock marker was not inherited');
  const manifest = validateManifest(JSON.parse(fs.readFileSync(options.manifest ?? path.join(root, 'docs/qa/raw-camera-manifest.json'), 'utf8')), extensions(fs.readFileSync(path.join(root, 'src/formats/raw.rs'), 'utf8')));
  const entry = manifest.cases.find(item => item.id === options.case);
  if (!entry || !entry.source.url?.startsWith('https://') || !digestPattern.test(entry.source.sha256 ?? '') || entry.source.license !== 'CC0-1.0') throw new Error('Acquisition requires known CC0 HTTPS source and independently pinned hash');
  const corpus = path.resolve(options.corpus);
  if (!corpus.startsWith('/home/') || fs.realpathSync(corpus) !== corpus) throw new Error('Acquisition requires an existing canonical corpus under /home');
  const rawDirectory = path.join(corpus, 'raw');
  if (fs.existsSync(rawDirectory) && fs.realpathSync(rawDirectory) !== rawDirectory) throw new Error('Acquisition refuses a symlink corpus directory');
  fs.mkdirSync(rawDirectory, { recursive: true });
  const destination = path.join(rawDirectory, entry.source.filename);
  if (fs.existsSync(destination)) {
    const existing = await inspectFile(destination, entry.source.sha256);
    console.log(JSON.stringify({ case: entry.id, status: existing.error ? 'failed' : 'cached', source_before_sha256: existing.actual, source_after_sha256: existing.actual, errors: existing.error ? [existing.error] : [] }));
    return existing.error ? 1 : 0;
  }
  const partial = `${destination}.partial-${crypto.randomUUID()}`;
  let fd;
  let partialCreated = false;
  let bytes = 0;
  try {
    const response = await fetch(entry.source.url, { signal: AbortSignal.timeout(120000) });
    if (!response.ok || !response.body) throw new Error(`Source HTTP ${response.status}`);
    if (Number(response.headers.get('content-length')) > sourceBudget || entry.source.size_bytes > sourceBudget) { console.log(JSON.stringify({ case: entry.id, status: 'needs_resource_scheduling', detail: 'Source exceeds acquisition budget; no support conclusion' })); await response.body.cancel(); return 1; }
    fd = fs.openSync(partial, 'wx');
    partialCreated = true;
    const hash = crypto.createHash('sha256');
    for await (const value of response.body) {
      bytes += value.byteLength;
      if (bytes > sourceBudget) throw new Error('needs_resource_scheduling: source exceeded acquisition budget');
      hash.update(value);
      let offset = 0;
      while (offset < value.byteLength) offset += fs.writeSync(fd, value, offset, value.byteLength - offset);
    }
    fs.closeSync(fd); fd = undefined;
    const digest = hash.digest('hex');
    if (!bytes || digest !== entry.source.sha256 || (entry.source.size_bytes !== null && bytes !== entry.source.size_bytes) || await hashFile(partial) !== entry.source.sha256) throw new Error('Source hash/size mismatch; cached/user sources remain untouched');
    fs.linkSync(partial, destination);
    console.log(JSON.stringify({ case: entry.id, status: 'acquired', bytes, source_before_sha256: digest, source_after_sha256: digest, provenance: entry.source.provenance }));
    return 0;
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
    if (partialCreated) fs.unlinkSync(partial);
  }
}

/** Parse a bounded CLI. Args: command arguments. Returns the command and options or throws. */
function parseArgs(args) {
  const [command, ...rest] = args;
  if (!['plan', 'run', 'acquire', 'self-test'].includes(command)) throw new Error('Usage: node scripts/raw-qualification.mjs plan|run|acquire|self-test [--manifest FILE] [--corpus DIR] [--case ID] [--bindings FILE] [--mode required|optional] [--config FILE --revision FULL_COMMIT --runner-sha256 HASH --output NEW_DIR]');
  const options = {};
  for (let index = 0; index < rest.length; index++) {
    const key = rest[index].replace(/^--/, '');
    if (key === 'lock-held') { options[key] = true; continue; }
    if (!rest[index].startsWith('--') || !['manifest', 'corpus', 'case', 'bindings', 'mode', 'config', 'revision', 'runner-sha256', 'output'].includes(key) || !rest[index + 1] || rest[index + 1].startsWith('--') || key in options) throw new Error('Invalid or duplicate CLI option');
    options[key] = rest[++index];
  }
  return { command, options };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { command, options } = parseArgs(process.argv.slice(2));
    process.exitCode = command === 'self-test' ? await selfTest() : command === 'acquire' ? await acquire(options) : await qualify(options, command);
  } catch (error) {
    console.error(JSON.stringify({ schema_version: 1, status: 'failed', error: error.message }));
    process.exitCode = 1;
  }
}
