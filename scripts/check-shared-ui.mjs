#!/usr/bin/env node
/** Check shared UI provenance. Args: consumer roots, or --at canonical root/commit then consumer root/commit pairs. Returns failure on drift. */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const args=process.argv.slice(2);
const pinned=args[0]==='--at';
let entries;
if(pinned){
  const pairs=args.slice(1);
  if(pairs.length<2||pairs.length%2)throw new Error('Expected canonical root/commit then consumer root/commit pairs');
  entries=Array.from({length:pairs.length/2},(_,i)=>({root:path.resolve(pairs[i*2]),commit:pairs[i*2+1]}));
  if(entries.some(entry=>!/^[a-f0-9]{40}$/.test(entry.commit)))throw new Error('Pinned checks require full commit SHAs');
}else entries=[root,...args].map(repository=>({root:path.resolve(repository)}));
const read=(entry,file)=>entry.commit
  ?execFileSync('git',['-C',entry.root,'show',`${entry.commit}:${file}`],{stdio:['ignore','pipe','pipe']})
  :fs.readFileSync(path.join(entry.root,file));
const manifestPath='vendor/oma-ui/manifest.json';
const manifest=JSON.parse(read(entries[0],manifestPath));
for(const entry of entries){
  const candidate=JSON.parse(read(entry,manifestPath));
  if(JSON.stringify(candidate)!==JSON.stringify(manifest))throw new Error(`Contract manifest drift: ${entry.root} ${entry.commit??'working tree'}`);
  for(const [file,expected] of Object.entries(manifest.sha256)){
    const actual=crypto.createHash('sha256').update(read(entry,`vendor/oma-ui/${file}`)).digest('hex');
    if(actual!==expected)throw new Error(`Shared UI drift: ${entry.root}/${file}`);
  }
}
console.log(`Oma UI contract ${manifest.contract} revision ${manifest.revision??1}: ${entries.length} repositories, ${Object.keys(manifest.sha256).length} identical sources each${pinned?' at exact commits':''}`);
