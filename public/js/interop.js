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

      const { font, lineHeight } = resolveTypography(textEl);
      let prepared = prepareWithSegments(source, font);

      const photo = () => document.querySelector(".profile-photo");

      const reflow = () => {
        const colWidth = textEl.clientWidth;
        if (colWidth <= 0) return;

        // Photo geometry expressed relative to the text container. The photo is
        // a circle (border-radius: 50%), so we model it as a circle inscribed in
        // its bounding box and exclude text from the circular region (plus a
        // uniform margin measured from the curve), NOT the square box. This lets
        // lines tuck into the corners the circle leaves empty.
        let cx = 0; // circle center x
        let cy = -Infinity; // circle center y
        let radius = 0; // circle radius (px)
        const ph = photo();
        if (ph) {
          const cr = textEl.getBoundingClientRect();
          const pr = ph.getBoundingClientRect();
          cx = (pr.left + pr.right) / 2 - cr.left;
          cy = (pr.top + pr.bottom) / 2 - cr.top;
          // Use the smaller half-extent so a non-square box still inscribes a
          // circle that fits within the rendered shape.
          radius = Math.min(pr.width, pr.height) / 2;
        }

        // The horizontal segment(s) available for text on the line whose top is
        // at `y`. When the photo's circle overlaps this band it returns BOTH the
        // gap to its left and the gap to its right (in reading order), so text
        // wraps around both sides of the photo. The excluded span is the circle's
        // width at the band row nearest the circle center (its widest point
        // within the band), expanded by PHOTO_MARGIN, so text hugs the curve.
        const segmentsForY = (y) => {
          const bandTop = y;
          const bandBottom = y + lineHeight;
          // Effective radius includes the breathing-room margin, measured from
          // the curve outward.
          const R = radius + PHOTO_MARGIN;
          // Vertical distance from the circle center to the nearest point of
          // this band (0 if the center's row falls inside the band).
          let dy;
          if (cy < bandTop) dy = bandTop - cy;
          else if (cy > bandBottom) dy = cy - bandBottom;
          else dy = 0;

          // No overlap with the (margin-expanded) circle on this band.
          if (radius <= 0 || dy >= R) return [{ x: 0, w: colWidth }];

          // Half-width of the excluded circular span at this band.
          const halfWidth = Math.sqrt(R * R - dy * dy);
          const excludeLeft = cx - halfWidth;
          const excludeRight = cx + halfWidth;

          const segs = [];
          const leftGap = excludeLeft; // from x=0 to the circle's left edge here
          if (leftGap >= MIN_LINE_WIDTH) segs.push({ x: 0, w: leftGap });

          const rightGap = colWidth - excludeRight; // circle's right edge to end
          if (rightGap >= MIN_LINE_WIDTH)
            segs.push({ x: excludeRight, w: rightGap });

          // Circle spans (nearly) the whole width on this band: no usable side
          // gap, so fall back to a full-width line that sits behind the photo.
          if (segs.length === 0) return [{ x: 0, w: colWidth }];
          return segs;
        };

        // Walk lines top-to-bottom. For each vertical band, fill every
        // available horizontal segment (left gap, then right gap) with
        // consecutive text before moving to the next band.
        const lines = [];
        let cursor = { segmentIndex: 0, graphemeIndex: 0 };
        let y = 0;
        let exhausted = false;
        // Guard against pathological loops.
        for (let i = 0; i < 2000 && !exhausted; i++) {
          const segs = segmentsForY(y);
          for (const seg of segs) {
            const range = layoutNextLineRange(
              prepared,
              cursor,
              Math.max(seg.w, 1),
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

        // Render: clear and lay out absolutely-positioned (still selectable)
        // line elements.
        textEl.textContent = "";
        textEl.style.position = "relative";
        const totalHeight = Math.max(
          y,
          lineHeight,
          cy + radius + PHOTO_MARGIN,
        );
        textEl.style.height = `${totalHeight}px`;
        const frag = document.createDocumentFragment();
        for (const ln of lines) {
          const div = document.createElement("div");
          div.className = "flow-line";
          div.style.position = "absolute";
          div.style.top = `${ln.y}px`;
          div.style.left = `${ln.x}px`;
          div.style.whiteSpace = "nowrap";
          div.style.lineHeight = `${lineHeight}px`;
          div.textContent = ln.text;
          frag.appendChild(div);
        }
        textEl.appendChild(frag);
      };

      // Reflow now, on photo drag, and on resize (recompute prepared text in
      // case the font/width context changed materially on resize).
      let resizeTimer = null;
      const onResize = () => {
        if (resizeTimer) cancelAnimationFrame(resizeTimer);
        resizeTimer = requestAnimationFrame(() => {
          const t = resolveTypography(textEl);
          prepared = prepareWithSegments(source, t.font);
          reflow();
        });
      };

      document.addEventListener(PHOTO_MOVE_EVENT, reflow);
      window.addEventListener("resize", onResize);

      // Run after fonts settle so measurements match rendered glyphs.
      if (document.fonts && document.fonts.ready) {
        document.fonts.ready.then(reflow);
      }
      reflow();
    })
    .catch((err) => {
      // If pretext fails to load, leave the plain text in place.
      console.error("text flow setup failed:", err);
    });
}
