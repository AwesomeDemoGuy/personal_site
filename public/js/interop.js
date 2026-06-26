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
// break their behavior or layout.
const FLOW_SELECTOR = "p, h1, h2, h3, h4, li, blockquote";

// Shared state across every flowed element on the page.
const flow = {
  pretext: null,
  photoEl: null,
  instances: [],
  frame: 0,
  scanFrame: 0,
};

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
  const { prepareWithSegments, layoutNextLineRange, materializeLineRange } =
    flow.pretext;

  if (el.dataset.flowText === undefined) {
    el.dataset.flowText = (el.textContent || "").trim();
  }
  const source = el.dataset.flowText;
  if (!source) return null;
  el.dataset.flowInit = "1";

  let { font, lineHeight } = resolveTypography(el);
  let prepared = prepareWithSegments(source, font);

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

    // Photo circle geometry relative to THIS element's box. The photo is a
    // circle (border-radius: 50%), modeled as the circle inscribed in its box;
    // text is excluded from the circular region (plus a uniform margin measured
    // from the curve), not the square box, so lines tuck into the corners.
    let cx = 0;
    let cy = -Infinity;
    let radius = 0;
    if (flow.photoEl) {
      const cr = el.getBoundingClientRect();
      const pr = flow.photoEl.getBoundingClientRect();
      cx = (pr.left + pr.right) / 2 - cr.left;
      cy = (pr.top + pr.bottom) / 2 - cr.top;
      radius = Math.min(pr.width, pr.height) / 2;
    }

    const key =
      colWidth +
      "|" +
      Math.round(cx) +
      "|" +
      Math.round(cy) +
      "|" +
      Math.round(radius) +
      "|" +
      lineHeight;

    const R = radius + PHOTO_MARGIN;
    const SINGLE_FULL = [{ x: 0, w: colWidth }];

    // The horizontal segment(s) available on the band whose top is at `y`. When
    // the circle overlaps the band, returns BOTH the left and right gaps so
    // text wraps around both sides; the excluded span is the circle's width at
    // the band row nearest its center. Segments are clamped to [0, colWidth].
    const segmentsForY = (y) => {
      const bandBottom = y + lineHeight;
      let dy;
      if (cy < y) dy = y - cy;
      else if (cy > bandBottom) dy = cy - bandBottom;
      else dy = 0;

      if (radius <= 0 || dy >= R) return SINGLE_FULL;

      const halfWidth = Math.sqrt(R * R - dy * dy);
      const excludeLeft = cx - halfWidth;
      const excludeRight = cx + halfWidth;

      if (excludeRight <= 0 || excludeLeft >= colWidth) return SINGLE_FULL;

      const segs = [];
      const leftEnd = excludeLeft < colWidth ? excludeLeft : colWidth;
      if (leftEnd >= MIN_LINE_WIDTH) segs.push({ x: 0, w: leftEnd });
      const rightStart = excludeRight > 0 ? excludeRight : 0;
      const rightGap = colWidth - rightStart;
      if (rightGap >= MIN_LINE_WIDTH) segs.push({ x: rightStart, w: rightGap });
      if (segs.length === 0) return SINGLE_FULL;
      return segs;
    };

    // Walk bands top-to-bottom, filling each available segment with consecutive
    // text. pretext does all the actual line breaking and measurement.
    const lines = [];
    let cursor = { segmentIndex: 0, graphemeIndex: 0 };
    let y = 0;
    let exhausted = false;
    for (let i = 0; i < 2000 && !exhausted; i++) {
      const segs = segmentsForY(y);
      for (let s = 0; s < segs.length; s++) {
        const seg = segs[s];
        const range = layoutNextLineRange(prepared, cursor, seg.w < 1 ? 1 : seg.w);
        if (range === null) {
          exhausted = true;
          break;
        }
        const line = materializeLineRange(prepared, range);
        lines.push({ text: line.text, x: seg.x, y });
        cursor = range.end;
      }
      y += lineHeight;
    }

    // Height is driven by the TEXT only, not the photo, so the photo floats
    // freely and never stretches the element it overlaps.
    const totalHeight = Math.max(y, lineHeight);
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
        prepared = prepareWithSegments(source, font);
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
  root.querySelectorAll(FLOW_SELECTOR).forEach((el) => {
    if (!isFlowable(el)) return;
    const inst = createInstance(el);
    if (inst) {
      flow.instances.push(inst);
      added += 1;
    }
  });
  flow.instances = flow.instances.filter((i) => i.isConnected());
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
      flow.photoEl = document.querySelector(".profile-photo");
      const root = document.querySelector("main.content") || document.body;

      scan(root);

      document.addEventListener(PHOTO_MOVE_EVENT, scheduleAll);

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
