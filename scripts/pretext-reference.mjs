// Rebuild deterministic fixtures from the frozen Pretext 0.0.8 oracle.
// These metrics deliberately do not claim to be browser font measurements.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import vm from 'node:vm';
import { gzipSync } from 'node:zlib';

const root = new URL('../', import.meta.url);
const fixtureDir = new URL('tests/fixtures/pretext/', root);
mkdirSync(fixtureDir, { recursive: true });
const source = readFileSync(new URL('oracle.mjs', fixtureDir), 'utf8');
const hash = createHash('sha256').update(source).digest('hex');
const cases = [
  '', '   ', 'Hello world! A paragraph with several words.',
  '  hello\tworld\r\n again  ', 'one\n\ntwo\n', '\n\n',
  'supercalifragilisticexpialidocious', 'abc-def—ghi',
  'ab\u00adcd\u00adef', 'one\u200btwo\u00a0three\u2060four',
  'a\tb\tlonger', 'abc    \n  xyz   ',
  'https://example.com/a?name=test&n=12 www.example.org/path',
  '12-34-56 1,234.56 1/2 15:30 +100% $25',
  '你好，世界。「こんにちは」한국어입니다。',
  'AGI 春天到了. بدأت الرحلة 🚀', 'שלום 123 world مرحبا،بالعالم',
  'a\u0301 e\u0308 👨‍👩‍👧‍👦 🇺🇸 👍🏽',
  'ภาษาไทยไม่มีช่องว่าง မြန်မာစာ ខ្មែរ',
  '((((hello)))) !!!!! foo_bar@host.example',
  "a\u202fb\ufeffc \"quoted\" 'text' …",
];
const results = [];
const measurement = text => {
  let width = 0;
  for (const ch of text) {
    if (/\p{M}/u.test(ch) || ch === '\u200d' || ch === '\u200b' || ch === '\u2060' || ch === '\ufeff' || ch === '\u00ad') continue;
    width += ch === ' ' || ch === '\u00a0' || ch === '\u202f' ? 4 : ch.codePointAt(0) > 0x2fff ? 16 : 7;
  }
  // Context-sensitive widths exercise pair and prefix preparation.
  width -= (text.match(/AV|To|12/g) || []).length * 0.5;
  return width;
};
for (const profile of ['default', 'chromium', 'safari']) {
  const measured = new Map();
  const context = {
    Intl, console,
    OffscreenCanvas: class {
      getContext() { return { font: '', measureText(text) { const width = measurement(text); measured.set(text, width); return { width }; } }; }
    },
    ...(profile === 'default' ? {} : { navigator: { userAgent: profile === 'safari' ? 'Safari/18' : 'Chrome/130', vendor: profile === 'safari' ? 'Apple Computer, Inc.' : 'Google Inc.' } }),
  };
  vm.createContext(context);
  vm.runInContext(source.replace(/export\s*\{[\s\S]*?\};?\s*$/, ''), context);
  context.setLocale('en');
  for (const text of cases) for (const whiteSpace of ['normal', 'pre-wrap']) for (const wordBreak of ['normal', 'keep-all']) for (const letterSpacing of [0, 1.5, -0.75]) {
    const options = { whiteSpace, wordBreak, letterSpacing };
    measured.clear(); context.clearCache();
    const prepared = context.prepareWithSegments(text, '16px Test', options);
    const normalized = context.analyzeText(text, context.getEngineProfile(), whiteSpace, wordBreak).normalized;
    const wordSegments = Array.from(new Intl.Segmenter('en', { granularity: 'word' }).segment(normalized), s => ({ text: s.segment, wordLike: s.isWordLike ?? false }));
    const graphemes = {};
    const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
    for (const s of [...wordSegments.map(s => s.text), ...prepared.segments, normalized]) graphemes[s] = Array.from(segmenter.segment(s), g => g.segment);
    const layouts = [0, 1, 14, 28, 64, 127.5, 320, 1e9].map(width => {
      const lines = [];
      let cursor = { segmentIndex: 0, graphemeIndex: 0 };
      while (true) {
        const range = context.layoutNextLineRange(prepared, cursor, width);
        if (range === null) break;
        if (range.end.segmentIndex === cursor.segmentIndex && range.end.graphemeIndex === cursor.graphemeIndex) throw Error('Oracle did not progress');
        lines.push(context.materializeLineRange(prepared, range));
        cursor = range.end;
      }
      return { width, lines, batch: context.layoutWithLines(prepared, width, 24), stats: context.measureLineStats(prepared, width), layout: context.layout(context.prepare(text, '16px Test', options), width, 24) };
    });
    results.push({ text, options, profile: context.getEngineProfile(), wordSegments, graphemes, measurements: Object.fromEntries(measured), prepared, layouts, naturalWidth: context.measureNaturalWidth(prepared) });
  }
}
writeFileSync(new URL('manifest.json', fixtureDir), JSON.stringify({ version: '0.0.8', sha256: hash, cases: results.length, metricBackend: 'deterministic; not browser fonts' }, null, 2) + '\n');
writeFileSync(new URL('cases.json.gz', fixtureDir), gzipSync(JSON.stringify(results) + '\n', { level: 9 }));
// Preserve generated Unicode data and punctuation sets from the exact oracle.
let data = '// Generated from the frozen Pretext 0.0.8 oracle. See tests/fixtures/pretext/LICENSE.\n';
const ctx = { Intl, OffscreenCanvas: class { getContext() { return { font: '', measureText: s => ({ width: measurement(s) }) }; } } };
vm.createContext(ctx); vm.runInContext(source.replace(/export\s*\{[\s\S]*?\};?\s*$/, ''), ctx);
data += `#[rustfmt::skip]\npub const LATIN1: [&str; 256] = ${JSON.stringify(ctx.latin1BidiTypes)};\n`;
data += `#[rustfmt::skip]\npub const BIDI_RANGES: &[(u32,u32,&str)] = &[${ctx.nonLatin1BidiRanges.map(r => `(${r[0]},${r[1]},${JSON.stringify(r[2])})`).join(',')}];\n`;
for (const [name, variable] of [['KINSOKU_START','kinsokuStart'],['KINSOKU_END','kinsokuEnd'],['LEFT_STICKY','leftStickyPunctuation'],['CLOSING_QUOTES','closingQuoteChars'],['NUMERIC_AFFIX','lineBreakNumericAffixRanges']]) {
  const value = ctx[variable];
  if (Array.isArray(value)) data += `#[rustfmt::skip]\npub const ${name}: &[u32] = &${JSON.stringify(value)};\n`;
  else data += `#[rustfmt::skip]\npub const ${name}: &str = ${JSON.stringify(Array.from(value).join('')).replace(/\\u([a-fA-F0-9]{4})/g, '\\u{$1}')};\n`;
}
writeFileSync(new URL('src/pretext/data.rs', root), data);
console.log(`Recorded ${results.length} cases; oracle sha256 ${hash}`);
