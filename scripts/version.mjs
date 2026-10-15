#!/usr/bin/env node
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = fileURLToPath(new URL('../', import.meta.url));
process.chdir(root);
const manifest = fs.readFileSync('Cargo.toml', 'utf8');
const current = manifest.match(/^version = "([^"]+)"/m)[1];
/**
 * Format the public nightly counter.
 * Args: Cargo version. Returns the release version with a four-digit counter.
 */
function publicVersion(version) {
  return version.replace(/(-nightly\.\d{8}\.)([1-9]\d{0,3})$/, (_, prefix, counter) => prefix + counter.padStart(4, '0'));
}
/**
 * Prepare consistent source version metadata without advertising an unpublished release.
 * Args: Cargo version and optional public nightly version. Returns nothing.
 */
function prepareVersion(cargoVersion, nightly) {
  const lock = fs.readFileSync('Cargo.lock', 'utf8');
  const ownPackage = /(\[\[package\]\]\nname = "omadesign"\nversion = ")[^"]+/g;
  if ([...lock.matchAll(ownPackage)].length !== 1) throw new Error('Expected one Omadesign lock entry');
  const updatedLock = lock.replace(ownPackage, `$1${cargoVersion}`);
  const updates = [
    ['Cargo.toml', manifest.replace(/^version = "[^"]+"/m, `version = "${cargoVersion}"`)],
    ['Cargo.lock', updatedLock],
  ];
  const inventory = 'vendor/rust-notices/licenses.json';
  if (fs.existsSync(inventory)) {
    const content = fs.readFileSync(inventory, 'utf8');
    if (!/"cargo_lock_sha256": "[a-f0-9]{64}"/.test(content)) throw new Error('Missing Rust notices lock hash');
    const hash = createHash('sha256').update(updatedLock).digest('hex');
    updates.push([inventory, content.replace(/"cargo_lock_sha256": "[a-f0-9]{64}"/, `"cargo_lock_sha256": "${hash}"`)]);
  }
  const website = 'site/src/release.ts';
  if (nightly && fs.existsSync(website)) {
    const content = fs.readFileSync(website, 'utf8');
    if (!/export const NIGHTLY_VERSION = "[^"]+";/.test(content)) throw new Error('Missing website nightly version');
    updates.push([website, content.replace(/export const NIGHTLY_VERSION = "[^"]+";/, `export const NIGHTLY_VERSION = "${nightly}";`)]);
  }
  for (const [path, content] of updates) fs.writeFileSync(path, content);
}
const [command = 'get', suppliedDate, suppliedCounter, extra] = process.argv.slice(2);
if (command === 'get') {
  console.log(publicVersion(current));
} else if (command === 'stable') {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(suppliedDate ?? '') || suppliedCounter !== undefined) throw new Error('Use stable MAJOR.MINOR.PATCH');
  const requested = suppliedDate.split('.').map(BigInt);
  const existing = current.split(/[-+]/)[0].split('.').map(BigInt);
  const difference = requested.findIndex((value, index) => value !== existing[index]);
  if ((difference >= 0 && requested[difference] < existing[difference]) || (difference < 0 && !current.includes('-'))) throw new Error('Stable version must advance the current version');
  if (execFileSync('git', ['tag', '--list', `v${suppliedDate}`], { encoding: 'utf8' }).trim()) throw new Error(`Tag v${suppliedDate} already exists`);
  prepareVersion(suppliedDate);
  console.log(suppliedDate);
} else if (command === 'nightly') {
  if (extra !== undefined) throw new Error('Too many nightly arguments');
  const date = suppliedDate ?? new Intl.DateTimeFormat('en-CA', { timeZone: 'America/New_York', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date()).replaceAll('-', '');
  const iso = date.replace(/^(\d{4})(\d{2})(\d{2})$/, '$1-$2-$3');
  if (!/^\d{8}$/.test(date) || Number.isNaN(Date.parse(iso)) || new Date(iso).toISOString().slice(0, 10) !== iso) throw new Error('Use a real date in YYYYMMDD form');
  const base = current.split(/[-+]/)[0];
  const prefix = `v${base}-nightly.${date}.`;
  const tags = execFileSync('git', ['tag', '--list', `${prefix}*`], { encoding: 'utf8' }).trim().split('\n').filter(Boolean);
  const counters = tags.map(tag => tag.slice(prefix.length)).filter(counter => /^\d{4}$/.test(counter)).map(Number);
  const counter = suppliedCounter === undefined ? Math.max(0, ...counters) + 1 : Number(suppliedCounter);
  if ((suppliedCounter !== undefined && !/^\d{1,4}$/.test(suppliedCounter)) || !Number.isInteger(counter) || counter < 1 || counter > 9999) throw new Error('Nightly counter must be 0001 through 9999');
  const cargoVersion = `${base}-nightly.${date}.${counter}`;
  const version = publicVersion(cargoVersion);
  if (tags.includes(`v${version}`)) throw new Error(`Tag v${version} already exists`);
  prepareVersion(cargoVersion, version);
  console.log(version);
} else {
  throw new Error('Usage: node scripts/version.mjs [get | stable MAJOR.MINOR.PATCH | nightly [YYYYMMDD [NNNN]]]');
}
