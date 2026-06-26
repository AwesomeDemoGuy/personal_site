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

export function setupTextFlow(textEl) {
  if (!textEl) return;

  // Capture the source text once; subsequent reflows rebuild from this.
  if (textEl.dataset.flowText === undefined) {
    textEl.dataset.flowText = (textEl.textContent || "").trim();
  }
  const source = textEl.dataset.flowText;
  if (!source) return;

  loadPretext()
    .then((pretext) => {
      const { prepareWithSegments, layoutNextLineRange, materializeLineRange } =
        pretext;

      let { font, lineHeight } = resolveTypography(textEl);
      // pretext's one-time analysis/measurement pass. Done once here (and again
      // only when the font changes on resize); the per-frame hot path below is
      // pure arithmetic over these cached segment widths.
      let prepared = prepareWithSegments(source, font);

      // Cache the photo element instead of re-querying every frame.
      let photoEl = document.querySelector(".profile-photo");

      // Set up the container once and keep a reusable pool of line elements, so
      // a reflow updates existing nodes in place rather than tearing down and
      // rebuilding the whole subtree each frame.
      textEl.textContent = "";
      textEl.style.position = "relative";
      const pool = [];
      let poolLineHeight = lineHeight;

      const acquireLine = (i) => {
        let el = pool[i];
        if (!el) {
          el = document.createElement("div");
          el.className = "flow-line";
          el.style.position = "absolute";
          el.style.whiteSpace = "nowrap";
          el.style.lineHeight = `${poolLineHeight}px`;
          pool[i] = el;
          textEl.appendChild(el);
        }
        return el;
      };

      // Compute the wrapped lines for the current geometry. Returns the line
      // list, total height, and a signature used to skip redundant renders.
      const computeLayout = () => {
        const colWidth = textEl.clientWidth;
        if (colWidth <= 0) return null;

        // Photo circle geometry relative to the text container. The photo is a
        // circle (border-radius: 50%), modeled as the circle inscribed in its
        // box; text is excluded from the circular region (plus a uniform margin
        // measured from the curve), not the square box, so lines tuck into the
        // corners the circle leaves empty.
        let cx = 0;
        let cy = -Infinity;
        let radius = 0;
        if (photoEl) {
          const cr = textEl.getBoundingClientRect();
          const pr = photoEl.getBoundingClientRect();
          cx = (pr.left + pr.right) / 2 - cr.left;
          cy = (pr.top + pr.bottom) / 2 - cr.top;
          radius = Math.min(pr.width, pr.height) / 2;
        }

        // Signature of the inputs that affect layout. If unchanged since the
        // last render, we can skip the whole pretext pass + DOM writes.
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

        // The horizontal segment(s) available on the band whose top is at `y`.
        // When the circle overlaps the band, returns BOTH the left and right
        // gaps (reading order) so text wraps around both sides; the excluded
        // span is the circle's width at the band row nearest its center.
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

          // If the excluded circle span doesn't intersect the text column at
          // all (photo dragged off to one side), use the full width.
          if (excludeRight <= 0 || excludeLeft >= colWidth) return SINGLE_FULL;

          const segs = [];
          // Left gap: column start -> circle's left edge, clamped to the column
          // so a photo sitting partly/fully outside never pushes text past the
          // left border.
          const leftEnd = excludeLeft < colWidth ? excludeLeft : colWidth;
          if (leftEnd >= MIN_LINE_WIDTH) segs.push({ x: 0, w: leftEnd });
          // Right gap: circle's right edge -> column end, clamped to the column
          // so text never starts left of 0 or runs past the right border.
          const rightStart = excludeRight > 0 ? excludeRight : 0;
          const rightGap = colWidth - rightStart;
          if (rightGap >= MIN_LINE_WIDTH)
            segs.push({ x: rightStart, w: rightGap });
          if (segs.length === 0) return SINGLE_FULL;
          return segs;
        };

        // Walk bands top-to-bottom, filling each available segment with
        // consecutive text. pretext does all the actual line breaking and
        // measurement (layoutNextLineRange / materializeLineRange).
        const lines = [];
        let cursor = { segmentIndex: 0, graphemeIndex: 0 };
        let y = 0;
        let exhausted = false;
        for (let i = 0; i < 2000 && !exhausted; i++) {
          const segs = segmentsForY(y);
          for (let s = 0; s < segs.length; s++) {
            const seg = segs[s];
            const range = layoutNextLineRange(
              prepared,
              cursor,
              seg.w < 1 ? 1 : seg.w,
            );
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

        // Height is driven by the TEXT only (where the last line ends), not by
        // the photo's position. The photo floats independently (it can be
        // dragged anywhere on the page), so tying the section height to it
        // would stretch the About section and prevent the photo from moving
        // below it.
        const totalHeight = Math.max(y, lineHeight);
        return { lines, totalHeight, key };
      };

      let lastKey = "";
      const render = (layout) => {
        if (layout.key === lastKey) return; // geometry unchanged: nothing to do
        lastKey = layout.key;

        const { lines, totalHeight } = layout;
        textEl.style.height = `${totalHeight}px`;

        // Update/reuse pooled line nodes in place.
        for (let i = 0; i < lines.length; i++) {
          const ln = lines[i];
          const el = acquireLine(i);
          if (el.style.display === "none") el.style.display = "";
          // Only touch the DOM when the value actually changed.
          const top = `${ln.y}px`;
          const left = `${ln.x}px`;
          if (el.style.top !== top) el.style.top = top;
          if (el.style.left !== left) el.style.left = left;
          if (el.textContent !== ln.text) el.textContent = ln.text;
        }
        // Hide any surplus nodes from a previous (longer) layout.
        for (let i = lines.length; i < pool.length; i++) {
          if (pool[i] && pool[i].style.display !== "none") {
            pool[i].style.display = "none";
          }
        }
      };

      // rAF-coalesced scheduling: many pointermove/photomove events within a
      // single frame collapse into one layout+render pass.
      let frame = 0;
      const schedule = () => {
        if (frame) return;
        frame = requestAnimationFrame(() => {
          frame = 0;
          const layout = computeLayout();
          if (layout) render(layout);
        });
      };

      // On resize the font/width context can change, so re-resolve typography
      // and re-run pretext's prepare pass before scheduling a render.
      let resizeQueued = false;
      const onResize = () => {
        if (resizeQueued) return;
        resizeQueued = true;
        requestAnimationFrame(() => {
          resizeQueued = false;
          const t = resolveTypography(textEl);
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
          lastKey = ""; // force a rebuild
          schedule();
        });
      };

      // Re-acquire the photo element if it wasn't present at setup time.
      if (!photoEl) photoEl = document.querySelector(".profile-photo");

      document.addEventListener(PHOTO_MOVE_EVENT, schedule);
      window.addEventListener("resize", onResize);

      // Run after fonts settle so measurements match rendered glyphs.
      if (document.fonts && document.fonts.ready) {
        document.fonts.ready.then(() => {
          lastKey = "";
          schedule();
        });
      }
      schedule();
    })
    .catch((err) => {
      // If pretext fails to load, leave the plain text in place.
      console.error("text flow setup failed:", err);
    });
}
