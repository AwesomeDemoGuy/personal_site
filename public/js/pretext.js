// Pretext text-measurement/layout library wrapper.
//
// Pretext (https://github.com/chenglou/pretext) is distributed as the npm
// package "@chenglou/pretext".
//
// In Docker builds this file is REPLACED automatically: the build stage runs
// `scripts/vendor-pretext.sh`, which installs the package and uses esbuild to
// emit the real library as a single self-contained ESM bundle here (overwriting
// the fallback below). See Dockerfile.
//
// For a real bundle during local `cargo leptos` development, run the same
// script yourself (requires node + npm):
//
//     ./scripts/vendor-pretext.sh
//
// Until then, the lightweight fallback below keeps the interop layer
// (public/js/interop.js) working. It approximates the subset of the pretext API
// the app uses, measuring with a canvas:
//   * prepare / layout                 (use case #1: paragraph height)
//   * prepareWithSegments              (use case #2: manual line layout)
//   * layoutNextLineRange / materializeLineRange
// The real library is far more accurate (bidi, grapheme segmentation, glue
// rules); this fallback is a greedy word-wrapper good enough for local dev.

const _ctxCache = new Map();

function ctxFor(font) {
  let ctx = _ctxCache.get(font);
  if (!ctx) {
    const canvas =
      typeof OffscreenCanvas !== "undefined"
        ? new OffscreenCanvas(1, 1)
        : document.createElement("canvas");
    ctx = canvas.getContext("2d");
    ctx.font = font;
    _ctxCache.set(font, ctx);
  }
  return ctx;
}

// ---- Use case #1: paragraph height -------------------------------------

export function prepare(text, font) {
  return { text: String(text ?? ""), font: String(font ?? "16px sans-serif") };
}

export function layout(prepared, maxWidth, lineHeight) {
  const { text, font } = prepared;
  if (!text) return { height: 0, lineCount: 0 };
  const ctx = ctxFor(font);
  const words = text.split(/\s+/);
  let lineCount = 1;
  let width = 0;
  for (const word of words) {
    const w = ctx.measureText(word + " ").width;
    if (width + w > maxWidth && width > 0) {
      lineCount += 1;
      width = w;
    } else {
      width += w;
    }
  }
  return { height: lineCount * lineHeight, lineCount };
}

// ---- Use case #2: manual line layout -----------------------------------

// The fallback models the prepared text as a flat list of "words" (with their
// trailing whitespace). A LayoutCursor here is simply a word index, carried in
// `segmentIndex`; `graphemeIndex` is unused (kept for API shape parity).

export function prepareWithSegments(text, font, _options) {
  const str = String(text ?? "");
  const f = String(font ?? "16px sans-serif");
  const ctx = ctxFor(f);
  // Split keeping track of words; collapse runs of whitespace to single space.
  const tokens = str.trim().split(/\s+/).filter((t) => t.length > 0);
  const words = tokens.map((t) => ({
    text: t,
    // Width of the word plus a single trailing space (used between words).
    width: ctx.measureText(t).width,
    spaceWidth: ctx.measureText(t + "\u00A0").width - ctx.measureText(t).width,
  }));
  return { words, font: f };
}

// Lay out one line starting at `start`, fitting into `maxWidth`. Returns a
// range { start, end, width } or null when the text is exhausted.
export function layoutNextLineRange(prepared, start, maxWidth) {
  const words = prepared.words;
  let i = start && Number.isInteger(start.segmentIndex) ? start.segmentIndex : 0;
  if (i >= words.length) return null;

  let width = 0;
  let count = 0;
  while (i < words.length) {
    const word = words[i];
    const add = (count === 0 ? 0 : word.spaceWidth) + word.width;
    if (count > 0 && width + add > maxWidth) break;
    width += add;
    i += 1;
    count += 1;
    // If even a single word overflows, still consume it (avoid infinite loop).
    if (count === 1 && width > maxWidth) break;
  }

  return {
    width,
    start: { segmentIndex: start.segmentIndex || 0, graphemeIndex: 0 },
    end: { segmentIndex: i, graphemeIndex: 0 },
  };
}

export function materializeLineRange(prepared, range) {
  const words = prepared.words;
  const from = range.start.segmentIndex || 0;
  const to = range.end.segmentIndex || 0;
  const text = words.slice(from, to).map((w) => w.text).join(" ");
  return { text, width: range.width, start: range.start, end: range.end };
}
