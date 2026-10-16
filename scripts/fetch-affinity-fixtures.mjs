#!/usr/bin/env node
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const manifest = JSON.parse(await fs.readFile(new URL('../tests/fixtures/affinity-native.json', import.meta.url), 'utf8'));
const root = path.resolve(process.argv[2] ?? '.artifacts/affinity-fixtures');
if (!root.startsWith('/home/')) throw new Error('Fixture output must remain under /home');
await fs.mkdir(root, { recursive: true });
for (const fixture of manifest) {
  const response = await fetch(fixture.url);
  if (!response.ok) throw new Error(`${fixture.name}: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const hash = createHash('sha256').update(bytes).digest('hex');
  if (hash !== fixture.sha256) throw new Error(`${fixture.name}: pinned SHA-256 mismatch`);
  await fs.writeFile(path.join(root, fixture.name), bytes);
}
await fs.writeFile(path.join(root, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log(`Fetched ${manifest.length} public MIT/CC0 Affinity fixtures into ${root}`);
