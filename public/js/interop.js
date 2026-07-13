// Browser interop for the profile photo: drag-to-move + pretext-driven text
// flow (text displaces around the photo as you drag it).
//
// Exposed to Rust via wasm-bindgen (see src/interop.rs). Uses Pointer Events so
// it works for both mouse and touch.
//
// The text-flow layout is computed by the pretext library
// (https://github.com/chenglou/pretext), use case #2 ("lay out the paragraph
// lines manually"): prepareWithSegments -> layoutNextLineRange ->
// materializeLineRange, narrowing the lines that overlap the photo's current
// rectangle. pretext is loaded with a dynamic absolute import so it resolves
// against the served site root (/js/pretext.js) rather than the wasm-bindgen
// snippets directory.

const PHOTO_MOVE_EVENT = "photomove";

// ---------------------------------------------------------------------------
// Draggable reset registry + navigation hook
// ---------------------------------------------------------------------------

// Every draggable registers a { el, reset } here. On a client-side navigation
// we restore each draggable to its default position (and discard any popped-out,
// now-orphaned floating element), so a new page never inherits the previous
// page's dragged-around layout.
const dragResets = [];

function registerDragReset(el, reset) {
  dragResets.push({ el, reset });
}

function resetAllDraggables() {
  for (let i = 0; i < dragResets.length; i++) {
    try {
      dragResets[i].reset();
    } catch (_) {
      /* ignore */
    }
  }
  // Forget draggables whose element has left the DOM.
  for (let i = dragResets.length - 1; i >= 0; i--) {
    if (!dragResets[i].el || !dragResets[i].el.isConnected) {
      dragResets.splice(i, 1);
    }
  }
}

// Invoke `cb` on client-side navigations. Leptos' router uses the History API,
// which emits no event for pushState/replaceState, so we wrap them once (and
// also listen for popstate for back/forward). The callback is deferred a frame
// so it runs after the router has swapped in the new page content.
let navHooked = false;
function onNavigate(cb) {
  if (navHooked) return;
  navHooked = true;
  const fire = () => requestAnimationFrame(cb);
  window.addEventListener("popstate", fire);
  for (const name of ["pushState", "replaceState"]) {
    const orig = history[name];
    history[name] = function (...args) {
      const ret = orig.apply(this, args);
      fire();
      return ret;
    };
  }
}

// ---------------------------------------------------------------------------
// Draggable photo
// ---------------------------------------------------------------------------

export function makeDraggable(element) {
  if (!element || element.dataset.draggableInit === "1") return;
  element.dataset.draggableInit = "1";

  let dragging = false;
  let startX = 0;
  let startY = 0;
  let offsetX = 0;
  let offsetY = 0;
  let baseX = 0;
  let baseY = 0;

  const apply = (x, y) => {
    element.style.transform = `translate(${x}px, ${y}px)`;
    // Notify any text-flow listeners that the photo rectangle changed.
    document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
  };

  const onPointerDown = (e) => {
    dragging = true;
    startX = e.clientX;
    startY = e.clientY;
    baseX = offsetX;
    baseY = offsetY;
    element.classList.add("dragging");
    element.setPointerCapture(e.pointerId);
    e.preventDefault();
  };

  const onPointerMove = (e) => {
    if (!dragging) return;
    offsetX = baseX + (e.clientX - startX);
    offsetY = baseY + (e.clientY - startY);
    apply(offsetX, offsetY);
  };

  const onPointerUp = (e) => {
    if (!dragging) return;
    dragging = false;
    element.classList.remove("dragging");
    try {
      element.releasePointerCapture(e.pointerId);
    } catch (_) {
      /* ignore */
    }
  };

  element.style.touchAction = "none";
  element.style.cursor = "grab";
  element.addEventListener("pointerdown", onPointerDown);
  element.addEventListener("pointermove", onPointerMove);
  element.addEventListener("pointerup", onPointerUp);
  element.addEventListener("pointercancel", onPointerUp);

  // Restore to the default (untranslated) position on navigation.
  registerDragReset(element, () => {
    offsetX = 0;
    offsetY = 0;
    element.style.transform = "";
    document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
  });
}

// A variant of makeDraggable for an element that starts nested inside other
// content (e.g. the certificate icon inside its card's link) but should behave
// as a free, page-level draggable once actually moved:
//   * A press that never moves past a small threshold is left alone, so the
//     element keeps its normal behavior (a click still follows its link).
//   * On the first real drag it is "popped out": reparented to <body> as an
//     absolutely-positioned element at its current on-screen spot. From then on
//     it is no longer tied to its original container's layout, and — being
//     outside the surrounding <a> — no longer acts as a hyperlink.
export function makeFloatingDraggable(element) {
  if (!element || element.dataset.draggableInit === "1") return;
  element.dataset.draggableInit = "1";

  const DRAG_THRESHOLD = 4; // px of movement before a press counts as a drag
  let dragging = false;
  let popped = false; // detached from its card yet?
  let moved = false; // has THIS gesture become a real drag?
  let lastDragEndAt = 0; // timestamp a real drag ended, to swallow its click
  let startX = 0;
  let startY = 0;
  let baseX = 0;
  let baseY = 0;
  let offsetX = 0;
  let offsetY = 0;
  let placeholder = null; // holds the icon's slot in the card while it floats
  let card = null; // the card element the icon was popped out of

  // Detach from the card/link and re-anchor to <body> at the same on-screen
  // position (document coords), so subsequent card reflows don't move it. A
  // same-size placeholder is left behind so the card keeps its dimensions, and
  // is remembered so the icon can dock back into the same slot later.
  const popOut = () => {
    const rect = element.getBoundingClientRect();
    card = element.parentNode;
    if (card) {
      placeholder = document.createElement("div");
      placeholder.style.width = `${rect.width}px`;
      placeholder.style.height = `${rect.height}px`;
      placeholder.style.flex = "0 0 auto";
      placeholder.setAttribute("aria-hidden", "true");
      card.insertBefore(placeholder, element);
    }
    element.style.position = "absolute";
    element.style.margin = "0";
    element.style.left = `${rect.left + window.scrollX}px`;
    element.style.top = `${rect.top + window.scrollY}px`;
    element.style.width = `${rect.width}px`;
    element.style.height = `${rect.height}px`;
    element.style.zIndex = "50";
    document.body.appendChild(element);
    popped = true;
    // Now free-floating: mark it so it counts as a text-flow obstacle, and
    // register it immediately (don't wait for the next scan) so text starts
    // wrapping around it on this very drag.
    element.dataset.floating = "1";
    if (!flow.photoEls.includes(element)) flow.photoEls.push(element);
  };

  // Is the icon's center currently over its origin card's box?
  const withinCard = () => {
    if (!card || !card.isConnected) return false;
    const c = card.getBoundingClientRect();
    const b = element.getBoundingClientRect();
    const bx = (b.left + b.right) / 2;
    const by = (b.top + b.bottom) / 2;
    return bx >= c.left && bx <= c.right && by >= c.top && by <= c.bottom;
  };

  // Return the icon to its card slot and clear all floating state, so it's tied
  // to the card again (and behaves as a normal link).
  const dock = () => {
    element.style.position = "";
    element.style.margin = "";
    element.style.left = "";
    element.style.top = "";
    element.style.width = "";
    element.style.height = "";
    element.style.zIndex = "";
    element.style.transform = "";
    delete element.dataset.floating;
    if (placeholder && placeholder.parentNode) {
      placeholder.parentNode.insertBefore(element, placeholder);
      placeholder.remove();
    }
    placeholder = null;
    card = null;
    popped = false;
    offsetX = 0;
    offsetY = 0;
    const idx = flow.photoEls.indexOf(element);
    if (idx !== -1) flow.photoEls.splice(idx, 1);
    // Reflow now that the belt is no longer a floating obstacle.
    document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
  };

  const apply = (x, y) => {
    element.style.transform = `translate(${x}px, ${y}px)`;
    document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
  };

  // Move/up are bound on `window` (not the element) for the duration of a drag.
  // Pointer capture can't be used here: popOut() reparents the element to
  // <body>, which implicitly releases capture — after which an element-bound
  // listener would only fire while the cursor is directly over the image, so a
  // fast drag would outrun it and stall, and a missed pointerup would leave the
  // element "stuck" to the cursor. Window listeners fire regardless of what's
  // under the pointer, so the drag tracks at any speed and always ends cleanly.
  const onPointerMove = (e) => {
    if (!dragging) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    // Ignore sub-threshold jitter so a click isn't misread as a drag.
    if (!moved && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    if (!moved) moved = true;
    if (!popped) popOut();
    offsetX = baseX + dx;
    offsetY = baseY + dy;
    apply(offsetX, offsetY);
    e.preventDefault();
  };

  const endDrag = () => {
    if (!dragging) return;
    dragging = false;
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", endDrag);
    if (!moved) return;
    // A real drag just ended: note the time so its click is swallowed, and if
    // the icon was dropped back over its card, dock it there.
    lastDragEndAt = performance.now();
    if (popped && withinCard()) dock();
  };

  const onPointerDown = (e) => {
    // Only a press that starts on the element begins a drag; passing the cursor
    // over the element while a button is held (a drag begun elsewhere) does not.
    dragging = true;
    moved = false; // fresh gesture: a plain click should still act as a link
    startX = e.clientX;
    startY = e.clientY;
    baseX = offsetX;
    baseY = offsetY;
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
    // Note: no preventDefault here — a plain click should still work as a link
    // until the press turns into a real drag.
  };

  // Swallow only the click fired right after a real drag ends (e.g. the drop
  // that docks the icon). A deliberate click later on a docked icon happens
  // well outside this window and still follows the link.
  const onClick = (e) => {
    if (performance.now() - lastDragEndAt < 400) {
      e.preventDefault();
      e.stopPropagation();
    }
  };

  element.style.touchAction = "none";
  element.style.cursor = "grab";
  element.addEventListener("pointerdown", onPointerDown);
  element.addEventListener("click", onClick);

  // On navigation, restore the default: if it was popped out (and possibly
  // orphaned when its card was removed), discard it — the about page renders a
  // fresh, docked icon when it mounts again. If still docked, just clear any
  // transform.
  registerDragReset(element, () => {
    if (popped) {
      const idx = flow.photoEls.indexOf(element);
      if (idx !== -1) flow.photoEls.splice(idx, 1);
      if (placeholder && placeholder.parentNode) placeholder.remove();
      placeholder = null;
      card = null;
      element.remove();
      document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
    } else {
      element.style.transform = "";
    }
    popped = false;
    moved = false;
    offsetX = 0;
    offsetY = 0;
  });
}

// ---------------------------------------------------------------------------
// pretext-driven text flow around the photo
// ---------------------------------------------------------------------------

// Horizontal/vertical breathing room kept between the photo and the text.
const PHOTO_MARGIN = 18;
// If a side gap is narrower than this, don't try to squeeze text into it.
const MIN_LINE_WIDTH = 64;

let pretextModulePromise = null;
function loadPretext() {
  if (!pretextModulePromise) {
    // Absolute path -> resolves against the served site root, not the
    // wasm-bindgen snippets dir (where a relative import would fail).
    pretextModulePromise = import("/js/pretext.js");
  }
  return pretextModulePromise;
}

// Resolve the canvas `font` shorthand and numeric line-height (px) from CSS so
// pretext measures the same text the browser would render.
function resolveTypography(el) {
  const cs = getComputedStyle(el);
  let font = cs.font;
  if (!font || font.trim() === "") {
    const style = cs.fontStyle || "normal";
    const weight = cs.fontWeight || "400";
    const size = cs.fontSize || "16px";
    const family = cs.fontFamily || "sans-serif";
    font = `${style} ${weight} ${size} ${family}`;
  }
  let lineHeight = parseFloat(cs.lineHeight);
  if (!Number.isFinite(lineHeight)) {
    lineHeight = (parseFloat(cs.fontSize) || 16) * 1.5;
  }
  return { font, lineHeight };
}

// Elements whose text we flow around the photo. Prose/reading text only — we
// deliberately skip interactive or structurally-laid-out elements (nav tabs,
// links, tag pills) so replacing their content with positioned lines doesn't
// break their behavior or layout. The weather widget's text span is included so
// the live weather flows around the photo too; its content arrives async (via
// Suspense), and the MutationObserver re-scan picks it up once resolved.
// `.cert-name` is the certificate label text — it flows in words independently
// of its icon sibling, which stays a plain, non-wrapping inline image.
// `pre` is the PGP public key block on the GPG page — its body is a single
// newline-free line, so it flows as one continuous chunk around the photo while
// the `-----BEGIN-----`/`-----END-----` armor lines stay on their own rows.
const FLOW_SELECTOR =
  "p, h1, h2, h3, h4, li, blockquote, .weather-widget span:not(.weather-loading), pre";

// Containers whose direct element children are atomic "chips" (link buttons,
// tech tags) that should flow around the photo as indivisible units — each chip
// stays whole and is never split apart.
const CHIP_CONTAINER_SELECTOR = ".links, .tech-tags, .about-email, .cert-cards";

// Shared state across every flowed element on the page.
const flow = {
  pretext: null,
  // Every element text should flow around (each modeled as a circle). The
  // profile photo plus any draggable certificate icons.
  photoEls: [],
  instances: [],
  frame: 0,
  scanFrame: 0,
};

// Collect the elements text should avoid: the profile photo and any certificate
// icon that has been popped out (is now a free-floating, page-level element).
// A docked icon still inside its card is NOT included — otherwise the card grid
// would try to flow around an icon that lives inside one of its own cards.
// Queried document-wide so popped-out icons (reparented to <body>) are found.
function refreshObstacles() {
  const els = [];
  const profile = document.querySelector(".profile-photo");
  if (profile) els.push(profile);
  document.querySelectorAll('.cert-icon[data-floating="1"]').forEach((e) => {
    els.push(e);
  });
  flow.photoEls = els;
}

// The obstacle circles (border-radius/inscribed) expressed relative to `el`'s
// box: for each obstacle, center (cx, cy) and radius (inscribed in its box).
// When `excludeBelts` is set, floating certificate icons are ignored — used by
// the certificate grid itself so it doesn't reflow around its own popped-out
// icon (which would drag that icon's card around).
function photoCirclesRelTo(el, excludeBelts) {
  const cr = el.getBoundingClientRect();
  const circles = [];
  for (let i = 0; i < flow.photoEls.length; i++) {
    const p = flow.photoEls[i];
    if (!p || !p.isConnected) continue;
    if (excludeBelts && p.classList.contains("cert-icon")) continue;
    const pr = p.getBoundingClientRect();
    if (pr.width <= 0 || pr.height <= 0) continue;
    circles.push({
      cx: (pr.left + pr.right) / 2 - cr.left,
      cy: (pr.top + pr.bottom) / 2 - cr.top,
      radius: Math.min(pr.width, pr.height) / 2,
    });
  }
  return circles;
}

// A cache-key fragment summarizing every obstacle circle's rounded geometry, so
// a relayout is skipped only when *no* obstacle (photo or belt) has moved.
function circlesKey(circles) {
  let k = "";
  for (let i = 0; i < circles.length; i++) {
    const c = circles[i];
    k +=
      Math.round(c.cx) + "," + Math.round(c.cy) + "," + Math.round(c.radius) + ";";
  }
  return k;
}

// Build a function that, for a band whose top is at `y` and which is
// `bandHeight` tall, returns the horizontal segment(s) available for content
// after excluding EVERY obstacle circle's region (plus PHOTO_MARGIN). Each
// overlapping circle contributes an excluded x-interval; the excluded intervals
// are merged and subtracted from [0, colWidth], leaving the free segments in
// reading order. Segments narrower than `minGap` are dropped. When nothing
// overlaps the band (or the row is fully blocked), returns a single full-width
// segment.
function buildSegmentsFn(colWidth, circles, bandHeight, minGap) {
  const SINGLE_FULL = [{ x: 0, w: colWidth }];
  return (y) => {
    const bandBottom = y + bandHeight;

    // Collect each overlapping circle's excluded [left, right] x-interval,
    // clamped to the column.
    const excludes = [];
    for (let i = 0; i < circles.length; i++) {
      const { cx, cy, radius } = circles[i];
      if (radius <= 0) continue;
      const R = radius + PHOTO_MARGIN;
      let dy;
      if (cy < y) dy = y - cy;
      else if (cy > bandBottom) dy = cy - bandBottom;
      else dy = 0;
      if (dy >= R) continue;

      const halfWidth = Math.sqrt(R * R - dy * dy);
      const left = cx - halfWidth;
      const right = cx + halfWidth;
      if (right <= 0 || left >= colWidth) continue;
      excludes.push([Math.max(0, left), Math.min(colWidth, right)]);
    }
    if (excludes.length === 0) return SINGLE_FULL;

    // Merge overlapping/adjacent exclusion intervals.
    excludes.sort((a, b) => a[0] - b[0]);
    const merged = [excludes[0].slice()];
    for (let i = 1; i < excludes.length; i++) {
      const last = merged[merged.length - 1];
      if (excludes[i][0] <= last[1]) {
        if (excludes[i][1] > last[1]) last[1] = excludes[i][1];
      } else {
        merged.push(excludes[i].slice());
      }
    }

    // Free segments are the gaps around/between the merged exclusions.
    const segs = [];
    let cursor = 0;
    for (let i = 0; i < merged.length; i++) {
      const [l, r] = merged[i];
      if (l - cursor >= minGap) segs.push({ x: cursor, w: l - cursor });
      if (r > cursor) cursor = r;
    }
    if (colWidth - cursor >= minGap) {
      segs.push({ x: cursor, w: colWidth - cursor });
    }
    if (segs.length === 0) return SINGLE_FULL;
    return segs;
  };
}

// A full-width single segment, used as the no-overlap fallback signature too.
function isFullWidthRow(segs, colWidth) {
  return (
    segs.length === 1 && segs[0].x === 0 && segs[0].w >= colWidth - 0.5
  );
}

// An element qualifies for flowing if it holds non-empty text and isn't a
// container of other elements or an interactive control. We require it to have
// no child *elements* (pure text) so we never clobber nested links/markup.
function isFlowable(el) {
  if (!el || el.dataset.flowInit === "1") return false;
  // No element children (text-only). Allows whitespace/text nodes only.
  if (el.children && el.children.length > 0) return false;
  // Skip anything inside interactive/structured regions.
  if (el.closest(".tabs, .links, .tech-tags, nav")) return false;
  const text = (el.textContent || "").trim();
  if (!text) return false;
  return true;
}

// Build a flow instance bound to a single text element. Each instance owns its
// own prepared text, line-node pool, and render cache, but reads the shared
// photo geometry so all elements wrap around the same circle.
function createInstance(el) {
  const {
    prepareWithSegments,
    layoutNextLineRange,
    materializeLineRange,
    measureNaturalWidth,
  } = flow.pretext;

  if (el.dataset.flowText === undefined) {
    el.dataset.flowText = (el.textContent || "").trim();
  }
  const source = el.dataset.flowText;
  if (!source) return null;
  el.dataset.flowInit = "1";

  // For prose we never split a word mid-grapheme (a side-gap must fit the widest
  // whole word). But a `pre` block like the PGP key is data, not prose: its long
  // base64 lines have no spaces, so treating them as unbreakable would force
  // every row to full width and the text would never flow around the photo.
  // Allow such blocks to break long lines at any grapheme so they wrap around
  // the image in every direction.
  const breakAnywhere = el.tagName === "PRE";

  let { font, lineHeight } = resolveTypography(el);
  // `pre-wrap` preserves the source's real line breaks as hard breaks. For the
  // PGP key that means the `-----BEGIN-----`/`-----END-----` armor lines stay on
  // their own rows, while the body (which is a single newline-free line) flows
  // as ONE continuous chunk — combined with `breakAnywhere` it wraps around the
  // photo character by character, with no ragged per-line remainders. For prose
  // with no embedded newlines this is a no-op.
  const whiteSpace = "pre-wrap";
  let prepared = prepareWithSegments(source, font, { whiteSpace });
  // Widest single word, used to gate side-gaps so pretext never has to break a
  // word mid-grapheme to fit. NOTE: measureNaturalWidth on the normal prepared
  // text returns the widest *forced line* — with no hard breaks that's the whole
  // paragraph, which is not what we want. So we measure a variant where every
  // space is a hard break (pre-wrap), making each word its own forced line;
  // then measureNaturalWidth returns the longest word's width.
  const widestWord = (f) =>
    measureNaturalWidth(
      prepareWithSegments(source.replace(/\s+/g, "\n"), f, {
        whiteSpace: "pre-wrap",
      }),
    );
  let minWordWidth = widestWord(font);

  // Vertical padding (preserved by the box model). Lines are positioned
  // absolutely, so we offset them by the top padding to sit where they would
  // naturally, and reserve the bottom padding in the height.
  const cs0 = getComputedStyle(el);
  const padTop = parseFloat(cs0.paddingTop) || 0;
  const padBottom = parseFloat(cs0.paddingBottom) || 0;

  el.textContent = "";
  el.style.position = "relative";
  const pool = [];
  let poolLineHeight = lineHeight;
  let lastKey = "";

  const acquireLine = (i) => {
    let node = pool[i];
    if (!node) {
      node = document.createElement("div");
      node.className = "flow-line";
      node.style.position = "absolute";
      node.style.whiteSpace = "nowrap";
      node.style.lineHeight = `${poolLineHeight}px`;
      pool[i] = node;
      el.appendChild(node);
    }
    return node;
  };

  const computeLayout = () => {
    const colWidth = el.clientWidth;
    if (colWidth <= 0) return null;

    // Obstacle circle geometry relative to THIS element's box. Each obstacle
    // (profile photo, draggable belt) is modeled as the circle inscribed in its
    // box; text is excluded from the circular regions (plus a uniform margin
    // measured from the curve), not the square boxes, so lines tuck into corners.
    const circles = photoCirclesRelTo(el);

    const key = colWidth + "|" + circlesKey(circles) + "|" + lineHeight;

    // A usable side-gap must fit the widest whole word; otherwise pretext would
    // have to break that word mid-grapheme to fill the gap. We skip gaps
    // narrower than this so words always wrap whole. (Still keep a small floor
    // so we don't try to use slivers when the longest word is tiny.)
    // Break-anywhere blocks (e.g. the PGP key `pre`) are exempt: their long
    // lines are meant to break to flow around the photo, so we only apply the
    // small floor and let pretext grapheme-break them into the side gaps.
    const minGap = breakAnywhere
      ? MIN_LINE_WIDTH
      : Math.max(MIN_LINE_WIDTH, Math.ceil(minWordWidth));
    const segmentsForY = buildSegmentsFn(colWidth, circles, lineHeight, minGap);

    // Walk bands top-to-bottom, filling each available segment with consecutive
    // text. Bands are sampled and rendered at the same on-screen y (offset by
    // the element's top padding) so the photo-overlap geometry matches where
    // the lines actually appear. pretext does all the line breaking/measurement.
    const lines = [];
    let cursor = { segmentIndex: 0, graphemeIndex: 0 };
    let y = 0;
    let exhausted = false;
    for (let i = 0; i < 2000 && !exhausted; i++) {
      const segs = segmentsForY(y + padTop);
      for (let s = 0; s < segs.length; s++) {
        const seg = segs[s];
        const range = layoutNextLineRange(prepared, cursor, seg.w < 1 ? 1 : seg.w);
        if (range === null) {
          exhausted = true;
          break;
        }
        const line = materializeLineRange(prepared, range);
        lines.push({ text: line.text, x: seg.x, y: y + padTop });
        cursor = range.end;
      }
      y += lineHeight;
    }

    // Height from the lowest line actually placed (each line.y already includes
    // padTop): content bottom = maxLineY + lineHeight, plus bottom padding.
    // Using the placed lines (not the loop counter, which over-counts the empty
    // band where exhaustion is detected) makes the height identical whether or
    // not the photo overlaps — so padded items keep constant height/spacing.
    let maxLineY = padTop;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].y > maxLineY) maxLineY = lines[i].y;
    }
    const totalHeight = maxLineY + lineHeight + padBottom;
    return { lines, totalHeight, key };
  };

  const render = (layout) => {
    if (layout.key === lastKey) return;
    lastKey = layout.key;

    const { lines, totalHeight } = layout;
    el.style.height = `${totalHeight}px`;

    for (let i = 0; i < lines.length; i++) {
      const ln = lines[i];
      const node = acquireLine(i);
      if (node.style.display === "none") node.style.display = "";
      const top = `${ln.y}px`;
      const left = `${ln.x}px`;
      if (node.style.top !== top) node.style.top = top;
      if (node.style.left !== left) node.style.left = left;
      if (node.textContent !== ln.text) node.textContent = ln.text;
    }
    for (let i = lines.length; i < pool.length; i++) {
      if (pool[i] && pool[i].style.display !== "none") {
        pool[i].style.display = "none";
      }
    }
  };

  return {
    el,
    isConnected: () => el.isConnected,
    relayout() {
      const layout = computeLayout();
      if (layout) render(layout);
    },
    refreshTypography() {
      const t = resolveTypography(el);
      if (t.font !== font) {
        font = t.font;
        prepared = prepareWithSegments(source, font, { whiteSpace });
        minWordWidth = widestWord(font);
      }
      if (t.lineHeight !== lineHeight) {
        lineHeight = t.lineHeight;
        poolLineHeight = lineHeight;
        for (let i = 0; i < pool.length; i++) {
          if (pool[i]) pool[i].style.lineHeight = `${lineHeight}px`;
        }
      }
      lastKey = "";
    },
  };
}

// Build a flow instance for a container of atomic chips (link buttons, tech
// tags). Each chip is an existing DOM element kept intact; we only absolutely
// position it. Chips are packed left-to-right into the circular gaps, wrapping
// to the next band when the current segment can't fit the next whole chip — so
// each chip flows around the photo as a single indivisible unit.
function createChipInstance(container) {
  if (container.dataset.flowInit === "1") return null;
  const chips = Array.from(container.children);
  if (chips.length === 0) return null;
  container.dataset.flowInit = "1";

  // Read the gap the CSS used between chips, and prepare the container to host
  // absolutely-positioned children without collapsing. Note: parse carefully so
  // an explicit `gap: 0` (e.g. `.about-email`, which must read as continuous
  // text) is honored rather than being treated as "unset" and defaulted.
  const csClient = getComputedStyle(container);
  const gapRaw = parseFloat(csClient.gap);
  const colGapRaw = parseFloat(csClient.columnGap);
  const gap = Number.isFinite(gapRaw)
    ? gapRaw
    : Number.isFinite(colGapRaw)
      ? colGapRaw
      : 10;
  container.style.position = "relative";

  // Cache each chip's natural (unwrapped) size once. Chips are inline-block so
  // their box size is intrinsic and stable.
  const sizes = chips.map((c) => {
    c.style.position = "absolute";
    c.style.top = "0px";
    c.style.left = "0px";
    const r = c.getBoundingClientRect();
    return { w: r.width, h: r.height };
  });
  const rowHeight = Math.max(...sizes.map((s) => s.h), 1);
  const widestChip = Math.max(...sizes.map((s) => s.w), 1);

  let lastKey = "";

  const relayout = () => {
    const colWidth = container.clientWidth;
    if (colWidth <= 0) return;

    // The certificate grid ignores floating belt icons as obstacles, so popping
    // a belt out doesn't shove its own (now-empty) card around. Other chip
    // containers (links, tech tags) still flow around the belt.
    const circles = photoCirclesRelTo(
      container,
      container.classList.contains("cert-cards"),
    );
    const key =
      colWidth + "|" + circlesKey(circles) + "|" + Math.round(rowHeight);
    if (key === lastKey) return;
    lastKey = key;

    // A usable segment must fit at least the widest chip, else chips can't be
    // placed there without overflowing — skip such slivers.
    const minGap = Math.max(MIN_LINE_WIDTH, Math.ceil(widestChip));
    const segmentsForY = buildSegmentsFn(colWidth, circles, rowHeight, minGap);

    let i = 0; // next chip to place
    let y = 0;
    let guard = 0;
    while (i < chips.length && guard++ < 2000) {
      const segs = segmentsForY(y);
      for (let s = 0; s < segs.length && i < chips.length; s++) {
        const seg = segs[s];
        // Pack as many whole chips as fit in this segment, left to right.
        let penX = seg.x;
        const segEnd = seg.x + seg.w;
        while (i < chips.length) {
          const cw = sizes[i].w;
          // First chip in a segment always goes (segment already >= widestChip);
          // subsequent chips need room for a preceding gap too.
          const needed = penX === seg.x ? cw : gap + cw;
          if (penX + needed > segEnd + 0.5) break;
          const x = penX === seg.x ? penX : penX + gap;
          const chip = chips[i];
          const left = `${Math.round(x)}px`;
          const top = `${Math.round(y)}px`;
          if (chip.style.left !== left) chip.style.left = left;
          if (chip.style.top !== top) chip.style.top = top;
          if (chip.style.display === "none") chip.style.display = "";
          penX = x + cw;
          i += 1;
        }
      }
      y += rowHeight + gap;
    }

    container.style.height = `${Math.max(y - gap, rowHeight)}px`;
  };

  return {
    el: container,
    isConnected: () => container.isConnected,
    relayout,
    refreshTypography() {
      // Re-measure chip sizes (font/zoom may have changed) by momentarily
      // clearing positioning influence is unnecessary: chips are absolutely
      // positioned with intrinsic size, so getBoundingClientRect stays valid.
      for (let k = 0; k < chips.length; k++) {
        const r = chips[k].getBoundingClientRect();
        sizes[k] = { w: r.width, h: r.height };
      }
      lastKey = "";
    },
  };
}

// Reflow every instance, coalesced to one animation frame.
function scheduleAll() {
  if (flow.frame) return;
  flow.frame = requestAnimationFrame(() => {
    flow.frame = 0;
    for (let i = 0; i < flow.instances.length; i++) {
      flow.instances[i].relayout();
    }
  });
}

// Discover new flowable elements under `root` and drop instances whose elements
// have left the DOM (e.g. after a client-side route change). Idempotent.
function scan(root) {
  let added = 0;
  // Prose text elements.
  root.querySelectorAll(FLOW_SELECTOR).forEach((el) => {
    if (!isFlowable(el)) return;
    const inst = createInstance(el);
    if (inst) {
      flow.instances.push(inst);
      added += 1;
    }
  });
  // Chip containers (link buttons, tech tags): each chip flows whole.
  root.querySelectorAll(CHIP_CONTAINER_SELECTOR).forEach((c) => {
    if (c.dataset.flowInit === "1") return;
    const inst = createChipInstance(c);
    if (inst) {
      flow.instances.push(inst);
      added += 1;
    }
  });
  flow.instances = flow.instances.filter((i) => i.isConnected());
  // Keep the obstacle set current: certificate icons may have been added by a
  // route change (or dropped from the DOM).
  refreshObstacles();
  if (added > 0) scheduleAll();
}

function scheduleScan(root) {
  if (flow.scanFrame) return;
  flow.scanFrame = requestAnimationFrame(() => {
    flow.scanFrame = 0;
    scan(root);
  });
}

// Entry point: flow all prose text under the main content region around the
// photo, and keep doing so as pages change (SPA navigation) and on resize.
export function setupAllTextFlow() {
  loadPretext()
    .then((pretext) => {
      flow.pretext = pretext;
      refreshObstacles();
      const root = document.querySelector("main.content") || document.body;

      scan(root);

      document.addEventListener(PHOTO_MOVE_EVENT, scheduleAll);

      // On client-side navigation, reset all draggables to their default spots.
      onNavigate(resetAllDraggables);

      let resizeQueued = false;
      window.addEventListener("resize", () => {
        if (resizeQueued) return;
        resizeQueued = true;
        requestAnimationFrame(() => {
          resizeQueued = false;
          for (let i = 0; i < flow.instances.length; i++) {
            flow.instances[i].refreshTypography();
          }
          scheduleAll();
        });
      });

      // Re-scan when the router swaps page content in/out. Our own line-node
      // writes also trigger this, but scan is idempotent (already-init elements
      // and generated .flow-line divs are ignored), so it settles immediately.
      const mo = new MutationObserver((muts) => {
        for (let i = 0; i < muts.length; i++) {
          if (muts[i].addedNodes.length || muts[i].removedNodes.length) {
            scheduleScan(root);
            break;
          }
        }
      });
      mo.observe(root, { childList: true, subtree: true });

      if (document.fonts && document.fonts.ready) {
        document.fonts.ready.then(() => {
          for (let i = 0; i < flow.instances.length; i++) {
            flow.instances[i].refreshTypography();
          }
          scheduleAll();
        });
      }

      scheduleAll();
    })
    .catch((err) => {
      console.error("text flow setup failed:", err);
    });
}
