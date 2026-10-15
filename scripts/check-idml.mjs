#!/usr/bin/env node
/**
 * Inspect a publishing export independently with Node and libxml2.
 * Args: output IDML; optional --master-source IDML --native OMA, --master-image FILE --unavailable-source DIR. Returns a JSON receipt or rejects broken references and payloads.
 */
import fs from 'node:fs';
import zlib from 'node:zlib';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';

function zipParts(file) {
  const bytes = fs.readFileSync(file), parts = new Map(), entries = [];
  const end = bytes.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  assert(end >= 0);
  const count = bytes.readUInt16LE(end + 10);
  let offset = bytes.readUInt32LE(end + 16);
  for (let i = 0; i < count; i++) {
    assert.equal(bytes.readUInt32LE(offset), 0x02014b50);
    const method = bytes.readUInt16LE(offset + 10), compressed = bytes.readUInt32LE(offset + 20);
    const size = bytes.readUInt32LE(offset + 24), nameLen = bytes.readUInt16LE(offset + 28);
    const extra = bytes.readUInt16LE(offset + 30), comment = bytes.readUInt16LE(offset + 32);
    const local = bytes.readUInt32LE(offset + 42), name = bytes.subarray(offset + 46, offset + 46 + nameLen).toString();
    assert(!parts.has(name));
    assert(!name.startsWith('/') && !name.includes('\\') && name.split('/').every(p => p && p !== '.' && p !== '..'));
    assert.equal(bytes.readUInt32LE(local), 0x04034b50);
    const start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28);
    assert.equal(bytes.subarray(local + 30, local + 30 + nameLen).toString(), name);
    const packed = bytes.subarray(start, start + compressed);
    const data = method === 0 ? packed : method === 8 ? zlib.inflateRawSync(packed) : assert.fail(`compression ${method}`);
    assert.equal(data.length, size);
    let crc = 0xffffffff;
    for (const b of data) { crc ^= b; for (let j = 0; j < 8; j++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0); }
    assert.equal((crc ^ 0xffffffff) >>> 0, bytes.readUInt32LE(offset + 16), `CRC ${name}`);
    parts.set(name, data); entries.push({name, method, size, local});
    offset += 46 + nameLen + extra + comment;
  }
  assert.equal(entries[0].name, 'mimetype'); assert.equal(entries[0].method, 0); assert.equal(entries[0].local, 0);
  assert.equal(parts.get('mimetype').toString(), 'application/vnd.adobe.indesign-idml-package');
  return {bytes, parts, entries};
}
function xpath(text, query) {
  const result = spawnSync('xmllint', ['--nonet', '--xpath', query, '-'], {input: text, encoding: 'utf8'});
  if ([10, 11].includes(result.status) && result.stderr.trim() === 'XPath set is empty') return '';
  assert.equal(result.status, 0, result.stderr); return result.stdout;
}
const attrs = (text, query) => [...xpath(text, query).matchAll(/="([^"]+)"/g)].map(m => m[1].replaceAll('&amp;', '&'));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const literal = value => value.includes("'") ? `concat(${value.split("'").map(p => `'${p}'`).join(`,"'",`)})` : `'${value}'`;
const inlineBytes = (xml, uri) => {
  const data = xpath(xml, `string(//Link[@LinkResourceURI=${literal(uri)}]/ancestor::*[self::Image or self::PDF or self::EPS][1]/Properties/Contents)`).replace(/\s/g, '');
  return data ? Buffer.from(data, 'base64') : undefined;
};
const file = process.argv[2], args = process.argv.slice(3);
const option = name => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const sourceFile = option('--master-source'), nativeFile = option('--native');
const externalImage = option('--master-image'), unavailableSource = option('--unavailable-source');
assert.equal(Boolean(sourceFile), Boolean(nativeFile), 'master inspection needs both source and native project');
assert.equal(Boolean(externalImage), Boolean(unavailableSource), 'source-folder inspection needs an image and the unavailable original source directory');
if (unavailableSource) assert(!fs.existsSync(unavailableSource), 'original source folder must be unavailable');
const {bytes, parts, entries} = zipParts(file);
assert(parts.has('META-INF/container.xml')); assert(parts.has('designmap.xml'));
const xmls = [...parts].filter(([name]) => name.endsWith('.xml'));
for (const [name, text] of xmls) {
  const result = spawnSync('xmllint', ['--nonet', '--noout', '-'], {input: text, encoding: 'utf8'});
  assert.equal(result.status, 0, `${name}: ${result.stderr}`);
}
const ids = new Set();
for (const [, text] of xmls) for (const id of attrs(text, '//@Self')) { assert(!ids.has(id), `duplicate ID ${id}`); ids.add(id); }
let refs = 0;
for (const [, text] of xmls) for (const id of attrs(text, '//@ParentStory | //@PreviousTextFrame | //@NextTextFrame | //@AppliedMaster | //@ItemLayer | //@AppliedParagraphStyle | //@AppliedCharacterStyle')) {
  if (id !== 'n') { assert(ids.has(id), `dangling reference ${id}`); refs++; }
}
for (const name of attrs(parts.get('designmap.xml'), '//@src')) assert(parts.has(name), `missing ${name}`);
const spreads = xmls.filter(([n]) => n.startsWith('Spreads/'));
const masters = xmls.filter(([n]) => n.startsWith('MasterSpreads/'));
const pages = spreads.reduce((sum, [, text]) => sum + Number(xpath(text, 'count(//Page)')), 0);
assert.equal(pages, 2); assert.equal(masters.length, 1);
for (const [key, value] of Object.entries({Top:9, InsideOrLeft:12, Bottom:15, OutsideOrRight:18})) {
  assert.equal(Number(xpath(parts.get('Resources/Preferences.xml'), `string(//DocumentPreference/@DocumentBleed${key}Offset)`)), value);
}
const stories = xmls.filter(([n]) => n.startsWith('Stories/'));
assert.equal(stories.length, 1);
const story = stories[0][1].toString();
assert(story.includes(sourceFile ? 'Café &amp; typography' : 'Edited café'));
assert(!story.includes('<Table') && !story.includes('<Note'));
assert(story.includes('CharacterStyle/Emphasis'));
const images = spreads.map(([, text]) => text.toString()).join('');
assert(images.includes('Links/mark.png')); assert(images.includes('Links/unavailable.png'));
const embedded = images.match(/<Contents><!\[CDATA\[([^]*?)\]\]><\/Contents>/);
assert(embedded && Buffer.from(embedded[1], 'base64').subarray(0, 8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
let masterChecks;
if (sourceFile) {
  const source = zipParts(sourceFile).parts, native = JSON.parse(fs.readFileSync(nativeFile));
  const layerIds = new Set(attrs(parts.get('designmap.xml'), '//Layer/@Self'));
  let layerRefs = 0;
  for (const [, text] of masters) for (const id of attrs(text, '//@ItemLayer')) {
    assert(layerIds.has(id), `missing master layer definition ${id}`);
    assert(native.doc.publishing.master_layers[id], `native source layer ${id}`); layerRefs++;
  }
  assert(layerRefs >= 2, 'fixture must exercise master ItemLayer references');
  const payloads = [];
  const nativeXmls = native.doc.publishing.masters.map(m => m.xml).concat(Object.values(native.doc.publishing.master_stories));
  for (const [name, text] of source) if (name.startsWith('MasterSpreads/')) {
    for (const uri of attrs(text, '//Link/@LinkResourceURI')) {
      const path = source.has(uri) ? uri : `Links/${uri.split('/').at(-1)}`;
      const expected = source.get(path) ?? (externalImage ? fs.readFileSync(externalImage) : undefined);
      assert(expected, `source master dependency ${uri} needs an explicit source-folder image`);
      const exported = parts.get(path) ?? masters.map(([, xml]) => inlineBytes(xml, uri)).find(Boolean);
      const nativePayload = native.doc.publishing.master_assets[path] ? Buffer.from(native.doc.publishing.master_assets[path]) : nativeXmls.map(xml => inlineBytes(xml, uri)).find(Boolean);
      assert(exported, `lost master payload ${uri}`);
      assert(exported.equals(expected), `changed master payload ${uri}`);
      assert(nativePayload?.equals(expected), `native payload ${uri}`);
      assert(masters.some(([, xml]) => attrs(xml, '//Link/@LinkResourceURI').includes(uri)), `lost master URI ${uri}`);
      if (!source.has(path)) assert(masters.some(([, xml]) => xpath(xml, `string(//Link[@LinkResourceURI=${literal(uri)}]/@StoredState)`).trim() === 'Embedded'), `loaded master link must be marked embedded: ${uri}`);
      payloads.push({uri, path, source:source.has(path) ? 'package' : 'source-folder', storage:parts.has(path) ? 'ZIP part' : 'inline Contents', bytes:expected.length, sha256:sha(expected)});
    }
  }
  assert(payloads.some(p => p.path === 'Links/logo.png'));
  assert.equal(Object.keys(native.doc.publishing.inherited_pages).length, 4);
  if (externalImage) {
    const previews = native.doc.layers.filter(l => l.kind.Raster && native.doc.publishing.inherited_layers.includes(l.id));
    assert.equal(previews.length, 2);
    assert(previews.every(l => l.visible === false && l.locked === true), 'hidden inherited rasters must stay hidden and locked');
  }
  for (const note of [
    'IDML Table is unsupported and was omitted; retain the source document.',
    'IDML Note is unsupported and was omitted; retain the source document.',
  ]) assert(native.doc.import_notes.includes(note), `saved conversion note: ${note}`);
  masterChecks = {nativeFile, nativeSha256:sha(fs.readFileSync(nativeFile)), nativeSchema:native.version,
    layerReferences:layerRefs, ownedPayloads:payloads, unavailableOriginalSource:unavailableSource,
    hiddenInheritedRasterStatePreserved:externalImage ? true : undefined, exactSavedTableAndNoteWarnings:true};
}
console.log(JSON.stringify({file, sha256:sha(bytes), zipEntries:entries.length, xmlParts:xmls.length, pages,
  stories:stories.length, masterSpreads:masters.length, uniqueIds:ids.size, checkedReferences:refs,
  embeddedPng:true, linkedUrisRetained:true, unsupportedTablesNotesAbsent:true, masterChecks, bytes:bytes.length}, null, 2));
