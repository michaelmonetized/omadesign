/**
 * Inspect Illustrator live type independently of Omadesign.
 * Args: input AI file, output JSON path, optional Poppler -bbox-layout HTML.
 * Returns: a JSON receipt with complete ATE story text and live artwork slots.
 * Uses qpdf for PDF stream decoding and Node for AI compression/ASCII85/ATE.
 * Numeric ATE key paths follow pinned VectorCraft b12a52f (MIT attribution is
 * retained by Omadesign in src/formats/illustrator/LICENSE-MIT).
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import zlib from 'node:zlib';

const MAX_PRIVATE = 256 << 20;
const MAX_TEXT = 64 << 20;
const [inputArgument, outputArgument, previewArgument] = process.argv.slice(2);
if (!inputArgument || !outputArgument) {
  throw new Error('Usage: node inspect-ai-live-text.mjs INPUT.ai OUTPUT.json [PREVIEW-TEXT.html]');
}
const input = path.resolve(inputArgument);
const output = path.resolve(outputArgument);
if (input === output || !output.endsWith('.json')) {
  throw new Error('Choose a separate .json output; original artwork is read-only');
}
const inputIdentity = fs.statSync(input);
const outputIdentity = fs.statSync(output, { throwIfNoEntry: false });
if (outputIdentity && inputIdentity.dev === outputIdentity.dev && inputIdentity.ino === outputIdentity.ino) {
  throw new Error('Output aliases original artwork; choose a separate .json output');
}

/** Read qpdf output. Args: flags. Returns bounded bytes without executing a shell. */
function qpdf(...flags) {
  return execFileSync('qpdf', [...flags, input], { maxBuffer: MAX_PRIVATE });
}

/** Read editing data. Args: none. Returns joined and decoded AI private streams. */
function editingData() {
  const pages = qpdf('--show-pages').toString();
  const firstPage = pages.match(/page 1: (\d+)/)?.[1];
  if (!firstPage) throw new Error('No PDF page');
  const object = id => qpdf(`--show-object=${id}`).toString();
  const owner = object(firstPage).match(/\/Illustrator (\d+) 0 R/)?.[1];
  if (!owner) throw new Error('No Illustrator PieceInfo owner');
  const privateId = object(owner).match(/\/Private (\d+) 0 R/)?.[1];
  if (!privateId) throw new Error('No Illustrator private dictionary');
  const dictionary = object(privateId);
  const prefix = /\/AIPrivateData1\s/.test(dictionary) ? 'AIPrivateData' : 'AIPDFPrivateData';
  const records = [...dictionary.matchAll(new RegExp(`/${prefix}(\\d+) (\\d+) 0 R`, 'g'))]
    .sort((a, b) => Number(a[1]) - Number(b[1]));
  if (!records.length || records.some((record, index) => Number(record[1]) !== index + 1)) {
    throw new Error('Missing or nonconsecutive private blocks');
  }
  const blocks = [];
  let length = 0;
  for (const record of records) {
    const bytes = qpdf(`--show-object=${record[2]}`, '--filtered-stream-data');
    length += bytes.length;
    if (length > MAX_PRIVATE) throw new Error('Private data exceeds 256 MiB');
    blocks.push(bytes);
  }
  const raw = Buffer.concat(blocks);
  let decoded = raw;
  for (const [marker, decode] of [
    ['%AI12_CompressedData', zlib.inflateSync],
    ['%AI24_ZStandard_Data', zlib.zstdDecompressSync],
  ]) {
    const at = raw.indexOf(marker);
    if (at >= 0) {
      const body = decode(raw.subarray(at + marker.length), { maxOutputLength: MAX_PRIVATE - at });
      decoded = Buffer.concat([raw.subarray(0, at), body]);
      break;
    }
  }
  return { bytes: decoded, privateBlocks: records.length, pdfPages: (pages.match(/page \d+:/g) || []).length };
}

/** Decode ASCII85. Args: encoded text. Returns bounded bytes or throws on malformed data. */
function ascii85(encoded) {
  const bytes = [];
  let group = [];
  function emit(count = 4) {
    const value = group.reduce((number, digit) => number * 85 + digit, 0);
    if (value > 0xffffffff) throw new Error('ASCII85 overflow');
    bytes.push(...[value >>> 24, (value >>> 16) & 255, (value >>> 8) & 255, value & 255].slice(0, count));
    group = [];
  }
  for (const char of encoded) {
    if (/\s/u.test(char)) continue;
    if (char === 'z' && !group.length) bytes.push(0, 0, 0, 0);
    else {
      const digit = char.charCodeAt(0) - 33;
      if (digit < 0 || digit > 84) throw new Error('Invalid ASCII85 digit');
      group.push(digit);
      if (group.length === 5) emit();
    }
    if (bytes.length > MAX_TEXT) throw new Error('Text data exceeds 64 MiB');
  }
  if (group.length === 1) throw new Error('Incomplete ASCII85 group');
  if (group.length) {
    const count = group.length - 1;
    while (group.length < 5) group.push(84);
    emit(count);
  }
  return Buffer.from(bytes);
}

/** Read ATE values. Args: decoded text bytes. Returns nested numeric-key dictionaries. */
function ateValues(bytes) {
  let cursor = 0;
  let budget = 4_000_000;
  const delimiter = byte => byte === undefined || byte <= 32 || '/<>[]()'.includes(String.fromCharCode(byte));
  const skip = () => { while (cursor < bytes.length && bytes[cursor] <= 32) cursor++; };
  function key() {
    skip();
    if (bytes[cursor++] !== 47) throw new Error('Expected ATE key');
    const start = cursor;
    while (!delimiter(bytes[cursor])) cursor++;
    return bytes.subarray(start, cursor).toString();
  }
  function string() {
    cursor++;
    const result = [];
    let depth = 0;
    let closed = false;
    while (cursor < bytes.length) {
      let byte = bytes[cursor++];
      if (byte === 41 && depth === 0) { closed = true; break; }
      if (byte === 40) depth++;
      if (byte === 41) depth--;
      if (byte === 92) {
        byte = bytes[cursor++];
        if (byte >= 48 && byte <= 55) {
          let number = byte - 48;
          for (let i = 0; i < 2 && bytes[cursor] >= 48 && bytes[cursor] <= 55; i++) {
            number = number * 8 + bytes[cursor++] - 48;
          }
          byte = number & 255;
        } else if (byte === 13 || byte === 10) {
          if (byte === 13 && bytes[cursor] === 10) cursor++;
          continue;
        } else {
          byte = ({ 110: 10, 114: 13, 116: 9, 98: 8, 102: 12 })[byte] ?? byte;
        }
      }
      result.push(byte);
    }
    if (!closed) throw new Error('Unterminated ATE string');
    const data = Buffer.from(result);
    if (data[0] === 254 && data[1] === 255) {
      if (data.length % 2) throw new Error('Odd UTF-16 text string');
      let text = '';
      for (let i = 2; i + 1 < data.length; i += 2) text += String.fromCharCode(data.readUInt16BE(i));
      return text;
    }
    if (data[0] === 255 && data[1] === 254) return data.subarray(2).toString('utf16le');
    return data.toString('latin1');
  }
  function value(depth = 0) {
    if (depth > 64 || --budget < 0) throw new Error('ATE nesting/value limit');
    skip();
    if (bytes[cursor] === 60 && bytes[cursor + 1] === 60) {
      cursor += 2;
      const dictionary = {};
      for (;;) {
        skip();
        if (bytes[cursor] === 62 && bytes[cursor + 1] === 62) { cursor += 2; return dictionary; }
        const name = key();
        dictionary[name] = value(depth + 1);
      }
    }
    if (bytes[cursor] === 91) {
      cursor++;
      const list = [];
      for (;;) {
        skip();
        if (bytes[cursor] === 93) { cursor++; return list; }
        list.push(value(depth + 1));
      }
    }
    if (bytes[cursor] === 47) return '/' + key();
    if (bytes[cursor] === 40) return string();
    const start = cursor;
    while (!delimiter(bytes[cursor])) cursor++;
    if (cursor === start) throw new Error('Expected ATE value');
    const word = bytes.subarray(start, cursor).toString();
    if (word === 'true') return true;
    if (word === 'false') return false;
    const number = Number(word);
    if (!Number.isFinite(number)) throw new Error('Invalid ATE number');
    return number;
  }
  const document = {};
  for (;;) {
    skip();
    if (cursor >= bytes.length) return document;
    const name = key();
    document[name] = value();
  }
}

/** Extract the main ATE engine. Args: editing operators. Returns parsed ATE and decoded size. */
function textEngine(postscript) {
  const at = postscript.indexOf('%AI11_BeginTextDocument');
  if (at < 0) throw new Error('No ATE text document');
  const tail = postscript.slice(at);
  const marker = tail.indexOf('/ASCII85Decode');
  const start = marker + '/ASCII85Decode'.length;
  const end = tail.indexOf('~>', start);
  if (marker < 0 || end < 0) throw new Error('Missing ATE ASCII85 section');
  const packed = tail.slice(start, end).split(/[\r\n]+/)
    .map((line, index) => index ? line.replace(/^%/, '') : line).join('').trim().replace(/^,/, '').trim();
  const bytes = ascii85(packed);
  return { document: ateValues(bytes), decodedBytes: bytes.length };
}

/** Find live artwork references. Args: AI operators. Returns scalar AI11Text slot dictionaries. */
function sourceSlots(postscript) {
  const firstLayer = postscript.indexOf('%AI5_BeginLayer');
  if (firstLayer < 0) throw new Error('No Illustrator artwork layers');
  const trailer = postscript.indexOf('%%PageTrailer', firstLayer);
  const artEnd = trailer < 0 ? postscript.length : trailer;
  const records = [];
  for (const match of postscript.matchAll(/(?:^|[\r\n])(%_)?\/AI11Text\s*:\s*([\s\S]*?);/g)) {
    const fields = match[2].replace(/%_/g, '');
    const number = (key, fallback) => Number(fields.match(new RegExp(`([0-9]+)\\s+/${key}\\s*,`))?.[1] ?? fallback);
    const storyIndex = number('StoryIndex', -1);
    if (storyIndex < 0) continue;
    records.push({ storyIndex, frameIndex: number('FrameIndex', 0), freeUndo: number('FreeUndo', 0),
      commented: Boolean(match[1]), offset: match.index, insideArtwork: match.index >= firstLayer && match.index < artEnd });
  }
  const outside = records.filter(record => !record.insideArtwork);
  if (outside.length) throw new Error(`Found ${outside.length} slot references outside artwork; inspect their role before using this oracle`);
  return records;
}

const normalize = content => content.replace(/\r+$/u, '').replace(/[\r\x03]/gu, '\n');
const words = content => content.trim() ? content.trim().split(/\s+/u) : [];
const original = fs.readFileSync(input);
if (original.length > 512 << 20) throw new Error('Source exceeds 512 MiB');
const editing = editingData();
const postscript = editing.bytes.toString('latin1');
const engine = textEngine(postscript);
const stories = (engine.document['1']?.['1'] ?? []).map((story, index) => {
  const raw = story['0']?.['0'];
  if (typeof raw !== 'string') throw new Error(`Story ${index} has no source content`);
  const content = normalize(raw);
  return { storyIndex: index, content, blank: !content.trim(), wordCount: words(content).length };
});
const slots = sourceSlots(postscript);
const activeSlots = slots.filter(slot => slot.freeUndo !== 1);
const pair = slot => `${slot.storyIndex}:${slot.frameIndex}`;
const plain = new Set(activeSlots.filter(slot => !slot.commented).map(pair));
const effectiveSlots = activeSlots.filter(slot => !slot.commented || !plain.has(pair(slot)));
const liveIds = new Set(effectiveSlots.map(slot => slot.storyIndex));
const liveStories = stories.filter(story => liveIds.has(story.storyIndex));
const orphans = stories.filter(story => !liveIds.has(story.storyIndex));
const seen = new Set();
const duplicateSlots = effectiveSlots.filter(slot => { const key = pair(slot); if (seen.has(key)) return true; seen.add(key); return false; });
const invalidSlots = effectiveSlots.filter(slot => slot.storyIndex >= stories.length);
let preview;
if (previewArgument) {
  const html = fs.readFileSync(previewArgument, 'utf8');
  const decode = text => text.replace(/&#(\d+);/g, (_, n) => String.fromCodePoint(Number(n)))
    .replace(/&#x([0-9a-f]+);/gi, (_, n) => String.fromCodePoint(parseInt(n, 16)))
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, '&');
  const pdfWords = [...html.matchAll(/<word\s[^>]*>([\s\S]*?)<\/word>/g)].map(match => decode(match[1]));
  const opening = pdfWords.slice(0, 28).join(' ');
  preview = { source: path.basename(previewArgument), extraction: 'Existing independent Poppler pdftotext -bbox-layout output',
    wordCount: pdfWords.length, opening,
    adobeNoPdfContentPlaceholder: /Adobe.*Illustrator.*File that was saved without PDF Content/u.test(pdfWords.join(' ')),
    artworkOracle: false };
}
const receipt = {
  source: path.basename(input), sourceBytes: original.length,
  sourceSha256: crypto.createHash('sha256').update(original).digest('hex'),
  method: 'Independent qpdf stream extraction + Node compression/ASCII85/ATE parser; no Omadesign code executed',
  normalization: 'Remove trailing CR; map internal CR and U+0003 to LF; count words by Unicode whitespace',
  privateBlocks: editing.privateBlocks, privateDecodedBytes: editing.bytes.length,
  ateDecodedBytes: engine.decodedBytes, pdfPages: editing.pdfPages,
  counts: { totalStories: stories.length, blankStories: stories.filter(story => story.blank).length,
    totalWords: stories.reduce((n, story) => n + story.wordCount, 0), totalSlots: slots.length,
    excludedFreeUndoSlots: slots.filter(slot => slot.freeUndo === 1).length,
    activeSlots: activeSlots.length, effectiveSlots: effectiveSlots.length, duplicateSlots: duplicateSlots.length,
    discardedCommentedCopies: activeSlots.length - effectiveSlots.length, invalidSlots: invalidSlots.length,
    liveStories: liveStories.length, nonemptyLiveStories: liveStories.filter(story => !story.blank).length,
    liveWords: liveStories.reduce((n, story) => n + story.wordCount, 0), orphanStories: orphans.length,
    orphanWords: orphans.reduce((n, story) => n + story.wordCount, 0) },
  expectedNativeTextObjects: liveStories.filter(story => !story.blank).length,
  expectedLiveStoryIndices: [...liveIds].sort((a, b) => a - b),
  liveStoryContent: Object.fromEntries(liveStories.map(story => [story.storyIndex, story.content])),
  multilineLiveStories: liveStories.filter(story => story.content.includes('\n')).map(story => ({
    storyIndex: story.storyIndex, lineBreakCount: (story.content.match(/\n/g) || []).length,
    content: story.content,
  })),
  liveContentDuplicates: [...new Set(liveStories.map(story => story.content))].map(content => ({
    content, storyIndices: liveStories.filter(story => story.content === content).map(story => story.storyIndex),
  })).filter(record => record.storyIndices.length > 1),
  activeSlots: effectiveSlots, duplicateSlots, invalidSlots,
  orphanStoryIndices: orphans.map(story => story.storyIndex),
  stories: stories.map(story => ({ ...story, live: liveIds.has(story.storyIndex) })), preview,
  proofBoundary: 'This receipt establishes source characters and live references. It does not prove native text placement, font/paint fidelity or renderer output.',
};
const temporaryOutput = path.join(path.dirname(output), `.${path.basename(output)}.${crypto.randomUUID()}.tmp`);
try {
  fs.writeFileSync(temporaryOutput, JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
  fs.renameSync(temporaryOutput, output);
} finally {
  fs.rmSync(temporaryOutput, { force: true });
}
process.stdout.write(JSON.stringify({ output: path.basename(output), sourceSha256: receipt.sourceSha256,
  counts: receipt.counts, expectedNativeTextObjects: receipt.expectedNativeTextObjects,
  orphanStoryIndices: receipt.orphanStoryIndices, multilineLiveStoryIndices: receipt.multilineLiveStories.map(story => story.storyIndex), liveContentDuplicateGroups: receipt.liveContentDuplicates.length, preview }, null, 2) + '\n');
