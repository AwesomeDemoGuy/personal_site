# Frozen Pretext reference

`oracle.mjs` is the original checked-in 0.0.8 production bundle; its SHA-256
appears in `manifest.json`. `interop.js` is the original site integration for
baseline comparisons. The upstream MIT license is retained here and distributed
with production assets at `public/licenses/pretext.txt`.

`cases.json.gz` records all 12 public APIs plus intermediate preparation data
for 756 cases. Widths come from a deterministic synthetic Canvas backend;
segmentation comes from Node's `Intl.Segmenter`. Native tests consume the recorded
word and grapheme boundaries instead of claiming native browser equivalence.

From the repository root, `node scripts/pretext-reference.mjs` regenerates the
fixtures, manifest, and bidi/punctuation tables in `src/pretext/data.rs`. Keep the
oracle frozen. If Node's Unicode or ICU version changes, review fixture changes
as a baseline update. Browser tests separately compare both engines using each
browser's real Intl and Canvas implementations.
