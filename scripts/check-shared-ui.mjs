#!/usr/bin/env node
/** Check shared UI provenance. Args: optional consumer repo roots. Returns failure on drift. */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root,'vendor/oma-ui/manifest.json'),'utf8'));
const roots = [root,...process.argv.slice(2)];
for (const repository of roots) {
  const candidate = JSON.parse(fs.readFileSync(path.join(repository,'vendor/oma-ui/manifest.json'),'utf8'));
  if (JSON.stringify(candidate) !== JSON.stringify(manifest)) throw new Error(`Contract manifest drift: ${repository}`);
  for (const [file,expected] of Object.entries(manifest.sha256)) {
    const actual = crypto.createHash('sha256').update(fs.readFileSync(path.join(repository,'vendor/oma-ui',file))).digest('hex');
    if (actual !== expected) throw new Error(`Shared UI drift: ${repository}/${file}`);
  }
}
console.log(`Oma UI contract ${manifest.contract}: ${roots.length} repositories, ${Object.keys(manifest.sha256).length} identical sources each`);
