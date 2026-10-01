// Browser interop for draggable photos, certificate icons, and blog images,
// with Pretext text flow around their shapes.
//
// Exposed to Rust via wasm-bindgen (see src/interop.rs). Mouse dragging starts
// immediately; touch dragging requires a hold so ordinary swipes scroll.
//
// The text-flow layout is computed by the pretext library
// (https://github.com/chenglou/pretext), use case #2 ("lay out the paragraph
// lines manually"): the plain and rich-inline streaming APIs route each line
// through the available gaps around circular photos and rectangular figures.
// pretext is loaded with a dynamic absolute import so it resolves
// against the served site root (/js/pretext.js) rather than the wasm-bindgen
// snippets directory.

const PHOTO_MOVE_EVENT = "photomove";

// Pointer events can arrive faster than the display refreshes. Move each image
// once per frame, immediately before the text-flow pass for that same frame.
const pendingMoves = new Map();
function queueDragMove(element, x, y) {
  pendingMoves.set(element, { x, y });
  scheduleAll();
}

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

let touchScrollLock = null;

function pageScrollPosition() {
  return touchScrollLock || { x: window.scrollX, y: window.scrollY };
}

// Cancelling touchmove alone does not reliably stop mobile viewport scrolling.
// Pin the body at its current viewport position for a held drag. Keep using the
// saved document offset when positioning floating images while the body is fixed.
function lockTouchScroll() {
  const body = document.body;
  const root = document.documentElement;
  const position = { x: window.scrollX, y: window.scrollY };
  const width = body.getBoundingClientRect().width;
  const saved = [];
  const set = (element, property, value) => {
    saved.push({ element, property, value: element.style.getPropertyValue(property),
      priority: element.style.getPropertyPriority(property) });
    element.style.setProperty(property, value);
  };
  touchScrollLock = position;
  set(root, "scroll-behavior", "auto");
  set(root, "overflow-anchor", "none");
  set(root, "overflow", "hidden");
  set(root, "touch-action", "none");
  set(body, "position", "fixed");
  set(body, "top", `${-position.y}px`);
  set(body, "left", `${-position.x}px`);
  set(body, "width", `${width}px`);

  return () => {
    // Restore the body before scrolling, with anchoring and smooth scrolling
    // still disabled so neither can adjust the restored viewport position.
    for (const entry of saved.slice().reverse()) {
      if (entry.element === root &&
        (entry.property === "scroll-behavior" || entry.property === "overflow-anchor")) continue;
      if (entry.value) entry.element.style.setProperty(entry.property, entry.value, entry.priority);
      else entry.element.style.removeProperty(entry.property);
    }
    touchScrollLock = null;
    window.scrollTo(position.x, position.y);
    // Let the queued text-flow pass finish before enabling scroll anchoring.
    requestAnimationFrame(() => {
      for (const entry of saved) {
        if (entry.element !== root ||
          (entry.property !== "scroll-behavior" && entry.property !== "overflow-anchor")) continue;
        if (entry.value) entry.element.style.setProperty(entry.property, entry.value, entry.priority);
        else entry.element.style.removeProperty(entry.property);
      }
    });
  };
}

// Keep native scrolling/pinch zoom available until a stationary touch has been
// held. Changing touch-action after the hold would not affect that gesture, so
// use a non-passive touchmove listener to claim only an activated touch drag.
function bindImageDrag(element, { start, move, end }) {
  const HOLD_MS = 450;
  const HOLD_SLOP = 8;
  let pointerId = null;
  let touchId = null;
  let holdTimer = null;
  let touchActive = false;
  let touchStart = null;
  let suppressClickUntil = 0;
  let unlockScroll = null;

  const reset = () => {
    clearTimeout(holdTimer);
    holdTimer = null;
    pointerId = null;
    touchId = null;
    touchActive = false;
    touchStart = null;
    element.classList.remove("touch-drag-ready");
    window.removeEventListener("pointermove", pointerMove);
    window.removeEventListener("pointerup", pointerEnd);
    window.removeEventListener("pointercancel", pointerEnd);
    window.removeEventListener("touchstart", extraTouch);
    window.removeEventListener("touchmove", touchMove);
    window.removeEventListener("touchend", touchEnd);
    window.removeEventListener("touchcancel", touchEnd);
    window.removeEventListener("blur", cancel);
    unlockScroll?.();
    unlockScroll = null;
  };
  const cancel = () => {
    const active = pointerId !== null || touchActive;
    if (touchActive) suppressClickUntil = performance.now() + 400;
    try {
      if (active) end();
    } finally {
      reset();
    }
  };
  const pointerMove = (event) => {
    if (event.pointerId === pointerId) move(event);
  };
  const pointerEnd = (event) => {
    if (event.pointerId === pointerId) cancel();
  };
  const extraTouch = (event) => {
    if (event.touches.length !== 1) cancel();
  };
  const touchMove = (event) => {
    const touch = Array.from(event.touches).find(t => t.identifier === touchId);
    if (!touch || event.touches.length !== 1 || !event.cancelable) {
      cancel();
      return;
    }
    if (!touchActive) {
      if (Math.hypot(touch.clientX - touchStart.clientX,
        touch.clientY - touchStart.clientY) > HOLD_SLOP) cancel();
      return;
    }
    event.preventDefault();
    move({ clientX: touch.clientX, clientY: touch.clientY,
      preventDefault: () => event.preventDefault() });
  };
  const touchEnd = (event) => {
    if (!Array.from(event.changedTouches).some(t => t.identifier === touchId)) return;
    if (touchActive && event.cancelable) event.preventDefault();
    cancel();
  };

  element.style.touchAction = "auto";
  element.addEventListener("pointerdown", event => {
    if (event.pointerType === "touch" || event.button !== 0 ||
      event.isPrimary === false || pointerId !== null || touchId !== null) return;
    pointerId = event.pointerId;
    window.addEventListener("pointermove", pointerMove);
    window.addEventListener("pointerup", pointerEnd);
    window.addEventListener("pointercancel", pointerEnd);
    window.addEventListener("blur", cancel);
    start(event);
  });
  element.addEventListener("touchstart", event => {
    if (event.touches.length !== 1 || touchId !== null || pointerId !== null) return;
    const touch = event.changedTouches[0];
    touchId = touch.identifier;
    touchStart = { clientX: touch.clientX, clientY: touch.clientY };
    window.addEventListener("touchstart", extraTouch, { passive: true });
    window.addEventListener("touchmove", touchMove, { passive: false });
    window.addEventListener("touchend", touchEnd, { passive: false });
    window.addEventListener("touchcancel", touchEnd, { passive: true });
    window.addEventListener("blur", cancel);
    holdTimer = setTimeout(() => {
      if (!element.isConnected) { cancel(); return; }
      touchActive = true;
      unlockScroll = lockTouchScroll();
      element.classList.add("touch-drag-ready");
      start({ ...touchStart, preventDefault() {} });
    }, HOLD_MS);
  }, { passive: true });
  element.addEventListener("contextmenu", event => {
    if (touchId !== null || performance.now() < suppressClickUntil) event.preventDefault();
  });
  element.addEventListener("click", event => {
    if (performance.now() < suppressClickUntil) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, true);
  element.addEventListener("dragstart", event => event.preventDefault());
  return reset;
}

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
    queueDragMove(element, x, y);
  };

  const onPointerDown = (e) => {
    dragging = true;
    startX = e.clientX;
    startY = e.clientY;
    baseX = offsetX;
    baseY = offsetY;
    element.classList.add("dragging");
    e.preventDefault();
  };

  const onPointerMove = (e) => {
    if (!dragging) return;
    offsetX = baseX + (e.clientX - startX);
    offsetY = baseY + (e.clientY - startY);
    apply(offsetX, offsetY);
  };

  const onPointerUp = () => {
    if (!dragging) return;
    dragging = false;
    element.classList.remove("dragging");
  };

  element.style.cursor = "grab";
  const resetGesture = bindImageDrag(element, {
    start: onPointerDown, move: onPointerMove, end: onPointerUp,
  });

  // Restore to the default (untranslated) position on navigation.
  registerDragReset(element, () => {
    resetGesture();
    pendingMoves.delete(element);
    dragging = false;
    element.classList.remove("dragging");
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
export function makeFloatingDraggable(element, { dockOnDrop = true } = {}) {
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
  let placeholder = null; // icon slot, or an invisible blog-image return marker
  let card = null; // the card element the icon was popped out of

  // Detach from the card/link and re-anchor to <body> at the same on-screen
  // position (document coords), so subsequent card reflows don't move it.
  // Certificate icons keep their card slot; blog images leave only a comment
  // marking their return position, so the article closes up behind them.
  const popOut = () => {
    const rect = element.getBoundingClientRect();
    const scroll = pageScrollPosition();
    const left = rect.left + scroll.x;
    const top = rect.top + scroll.y;
    const isBlogImage = element.classList.contains("blog-image");
    card = element.parentNode;
    if (card) {
      if (isBlogImage) {
        placeholder = document.createComment("blog-image-origin");
      } else {
        placeholder = document.createElement("div");
        placeholder.style.width = `${rect.width}px`;
        placeholder.style.height = `${rect.height}px`;
        placeholder.style.flex = "0 0 auto";
        placeholder.setAttribute("aria-hidden", "true");
      }
      card.insertBefore(placeholder, element);
    }
    element.style.position = "absolute";
    element.style.margin = "0";
    element.style.left = `${left}px`;
    element.style.top = `${top}px`;
    element.style.width = `${rect.width}px`;
    element.style.height = `${rect.height}px`;
    element.style.zIndex = "50";
    document.body.appendChild(element);
    if (isBlogImage && card) {
      card.classList.toggle("markdown-image-group-empty", card.children.length === 0);
      // Collapsing the source row can adjust browser scroll anchoring. Keep the
      // image at the same viewport position as it detaches from that row.
      const currentScroll = pageScrollPosition();
      element.style.left = `${rect.left + currentScroll.x}px`;
      element.style.top = `${rect.top + currentScroll.y}px`;
    }
    popped = true;
    // Now free-floating: mark it so it counts as a text-flow obstacle, and
    // register it immediately (don't wait for the next scan) so text starts
    // wrapping around it on this very drag.
    element.dataset.floating = "1";
    if (!flow.obstacleEls.includes(element)) flow.obstacleEls.push(element);
  };

  // Is the icon's center currently over its origin card's box?
  const withinCard = () => {
    if (!card || !card.isConnected) return false;
    const c = card.getBoundingClientRect();
    const b = element.getBoundingClientRect();
    const bx = (b.left + b.right) / 2;
    const by = (b.top + b.bottom) / 2;
    if (element.classList.contains("blog-image")) {
      // The original row has collapsed, so its actual height is no longer a
      // useful drop target (and may contain all the following prose). Treat
      // the centered image slot at the group's start as a virtual return area.
      const width = Math.min(b.width, c.width);
      const height = b.height * width / b.width;
      return Math.abs(bx - (c.left + c.right) / 2) <= Math.max(24, width / 4) &&
        Math.abs(by - (c.top + height / 2)) <= Math.max(24, height / 4);
    }
    return bx >= c.left && bx <= c.right && by >= c.top && by <= c.bottom;
  };

  // Return the icon to its card slot and clear all floating state, so it's tied
  // to the card again (and behaves as a normal link).
  const dock = () => {
    pendingMoves.delete(element);
    element.style.position = "";
    element.style.margin = "";
    element.style.left = "";
    element.style.top = "";
    element.style.width = "";
    element.style.height = "";
    element.style.zIndex = "";
    element.style.transform = "";
    delete element.dataset.floating;
    card?.classList.remove("markdown-image-group-empty");
    if (placeholder && placeholder.parentNode) {
      placeholder.parentNode.insertBefore(element, placeholder);
      placeholder.remove();
    }
    placeholder = null;
    card = null;
    popped = false;
    offsetX = 0;
    offsetY = 0;
    const idx = flow.obstacleEls.indexOf(element);
    if (idx !== -1) flow.obstacleEls.splice(idx, 1);
    // Reflow now that the belt is no longer a floating obstacle.
    document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
  };

  const apply = (x, y) => {
    queueDragMove(element, x, y);
  };

  // Gesture tracking stays on `window` after the element moves to <body>.
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
    if (!moved) {
      moved = true;
      element.classList.add("dragging");
      element.style.cursor = "grabbing";
    }
    if (!popped) popOut();
    offsetX = baseX + dx;
    offsetY = baseY + dy;
    apply(offsetX, offsetY);
    e.preventDefault();
  };

  const endDrag = () => {
    if (!dragging) return;
    dragging = false;
    element.classList.remove("dragging");
    element.style.cursor = "grab";
    if (!moved) return;
    // A real drag just ended: note the time so its click is swallowed, and if
    // the icon was dropped back over its card, dock it there.
    lastDragEndAt = performance.now();
    // Hit testing on release must use the final pointer position, even when
    // pointerup arrives before the queued animation frame.
    if (pendingMoves.delete(element)) {
      element.style.transform = `translate(${offsetX}px, ${offsetY}px)`;
    }
    if (dockOnDrop && popped && withinCard()) dock();
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

  element.style.cursor = "grab";
  const resetGesture = bindImageDrag(element, {
    start: onPointerDown, move: onPointerMove, end: endDrag,
  });
  element.addEventListener("click", onClick);

  // On navigation, restore the default: if it was popped out (and possibly
  // orphaned when its card was removed), discard it — the about page renders a
  // fresh, docked icon when it mounts again. If still docked, just clear any
  // transform.
  registerDragReset(element, () => {
    resetGesture();
    pendingMoves.delete(element);
    dragging = false;
    element.classList.remove("dragging");
    element.style.cursor = "grab";
    if (popped) {
      if (element.classList.contains("blog-image") && card?.isConnected) {
        dock();
        moved = false;
        return;
      }
      const idx = flow.obstacleEls.indexOf(element);
      if (idx !== -1) flow.obstacleEls.splice(idx, 1);
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
  "p, h1, h2, h3, h4, h5, h6, li, blockquote, .weather-widget span:not(.weather-loading), pre";

// Containers whose direct element children are atomic "chips" (link buttons,
// tech tags) that should flow around the photo as indivisible units — each chip
// stays whole and is never split apart.
const CHIP_CONTAINER_SELECTOR = ".links, .tech-tags, .about-email, .cert-cards";

// Shared state across every flowed element on the page.
const flow = {
  pretext: null,
  // Circular photos and floating certificate icons, plus rectangular blog figures.
  obstacleEls: [],
  instances: [],
  frame: 0,
  scanFrame: 0,
  imageGroups: [],
  imageObserver: null,
  obstacleRects: null,
  layingOut: false,
  markdownBodies: new WeakMap(),
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
  // Docked figures already occupy their own row in normal document flow.
  // Only detached figures can overlap prose and need exclusion geometry.
  document.querySelectorAll('.blog-image[data-floating="1"]').forEach((e) => {
    els.push(e);
  });
  flow.obstacleEls = els;
}

// Obstacles expressed relative to el: circles have a center/radius, and blog
// figures have their full rectangular bounds.
// When `excludeBelts` is set, floating certificate icons are ignored — used by
// the certificate grid itself so it doesn't reflow around its own popped-out
// icon (which would drag that icon's card around).
function documentRect(el) {
  const rect = el.getBoundingClientRect();
  const scroll = pageScrollPosition();
  return {
    left: rect.left + scroll.x,
    right: rect.right + scroll.x,
    top: rect.top + scroll.y,
    bottom: rect.bottom + scroll.y,
    width: rect.width,
    height: rect.height,
  };
}

function obstaclesRelTo(el, excludeBelts, minimumHeight = 0) {
  const cr = documentRect(el);
  const circles = [];
  for (let i = 0; i < flow.obstacleEls.length; i++) {
    const p = flow.obstacleEls[i];
    if (!p || !p.isConnected) continue;
    if (excludeBelts && p.classList.contains("cert-icon")) continue;
    if (p.contains(el)) continue;
    const pr = flow.obstacleRects?.get(p) || documentRect(p);
    if (pr.width <= 0 || pr.height <= 0) continue;
    // Obstacles above or outside the column cannot affect any of its lines.
    if (pr.bottom + PHOTO_MARGIN <= cr.top ||
        pr.right + PHOTO_MARGIN <= cr.left ||
        pr.left - PHOTO_MARGIN >= cr.right) continue;
    if (p.classList.contains("blog-image")) {
      circles.push({
        left: pr.left - cr.left,
        right: pr.right - cr.left,
        top: pr.top - cr.top,
        bottom: pr.bottom - cr.top,
      });
      continue;
    }
    circles.push({
      cx: (pr.left + pr.right) / 2 - cr.left,
      cy: (pr.top + pr.bottom) / 2 - cr.top,
      radius: Math.min(pr.width, pr.height) / 2,
    });
  }
  // A shape below the block cannot affect its natural layout. Once a shape
  // intersects, retain those farther down too: wrapping can extend the block
  // into them. This also makes the cache key stable for distant text blocks.
  return circles.some((c) => (c.top !== undefined
    ? c.top - PHOTO_MARGIN : c.cy - c.radius - PHOTO_MARGIN) < Math.max(cr.height, minimumHeight))
    ? circles : [];
}

// A cache-key fragment summarizing every obstacle's rounded geometry, so
// a relayout is skipped only when *no* obstacle (photo or belt) has moved.
function obstaclesKey(circles) {
  let k = "";
  for (let i = 0; i < circles.length; i++) {
    const c = circles[i];
    if (c.left !== undefined) {
      k += [c.left, c.right, c.top, c.bottom].map((n) => Math.round(n * 10) / 10).join(",") + ";";
      continue;
    }
    k +=
      Math.round(c.cx) + "," + Math.round(c.cy) + "," + Math.round(c.radius) + ";";
  }
  return k;
}

// Build a function that, for a band whose top is at `y` and which is
// `bandHeight` tall, returns the horizontal segment(s) available for content
// after excluding every obstacle's region (plus PHOTO_MARGIN). Each
// overlapping shape contributes an excluded x-interval; the excluded intervals
// are merged and subtracted from [0, colWidth], leaving the free segments in
// reading order. Segments narrower than `minGap` are dropped. When nothing
// overlaps the band, returns a full-width segment. A fully blocked band returns
// no segments so callers advance vertically without consuming text.
function buildSegmentsFn(colWidth, circles, bandHeight, minGap) {
  const SINGLE_FULL = [{ x: 0, w: colWidth }];
  return (y) => {
    const bandBottom = y + bandHeight;

    // Collect each overlapping circle's excluded [left, right] x-interval,
    // clamped to the column.
    const excludes = [];
    for (let i = 0; i < circles.length; i++) {
      const obstacle = circles[i];
      if (obstacle.left !== undefined) {
        if (bandBottom <= obstacle.top - PHOTO_MARGIN || y >= obstacle.bottom + PHOTO_MARGIN) continue;
        const left = Math.max(0, obstacle.left - PHOTO_MARGIN);
        const right = Math.min(colWidth, obstacle.right + PHOTO_MARGIN);
        if (right > left) excludes.push([left, right]);
        continue;
      }
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
    return segs;
  };
}

// An element qualifies for flowing if it holds non-empty text and isn't a
// container of other elements or an interactive control. We require it to have
// no child *elements* (pure text) so we never clobber nested links/markup.
function isFlowable(el) {
  if (!el || el.dataset.flowInit === "1") return false;
  if (el.closest(".markdown-body, .flow-source, .flow-line")) return false;
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
  let lastWidth = 0;

  const acquireLine = (i) => {
    let node = pool[i];
    if (!node) {
      node = document.createElement("span");
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
    // A new width/font can grow the block into obstacles below its old bounds.
    const circles = obstaclesRelTo(el, false, !lastKey || lastWidth !== colWidth ? Infinity : 0);

    const key = colWidth + "|" + obstaclesKey(circles) + "|" + lineHeight;
    if (key === lastKey) return null;

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
    return { lines, totalHeight, key, width: colWidth };
  };

  const render = (layout) => {
    if (layout.key === lastKey) return;
    lastKey = layout.key;
    lastWidth = layout.width;

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
      font = t.font;
      prepared = prepareWithSegments(source, font, { whiteSpace });
      minWordWidth = widestWord(font);
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
  let lastWidth = 0;

  const relayout = () => {
    const colWidth = container.clientWidth;
    if (colWidth <= 0) return;

    // The certificate grid ignores floating belt icons as obstacles, so popping
    // a belt out doesn't shove its own (now-empty) card around. Other chip
    // containers (links, tech tags) still flow around the belt.
    const circles = obstaclesRelTo(
      container,
      container.classList.contains("cert-cards"),
      !lastKey || lastWidth !== colWidth ? Infinity : 0,
    );
    const key =
      colWidth + "|" + obstaclesKey(circles) + "|" + Math.round(rowHeight);
    if (key === lastKey) return;
    lastKey = key;
    lastWidth = colWidth;

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

// Markdown is an inner_html subtree owned by this adapter. Preserve a canonical
// copy for assistive technology, and render separate, selectable visual lines.
function prepareMarkdown(root) {
  root.querySelectorAll(".markdown-body").forEach((body) => {
    const marker = flow.markdownBodies.get(body);
    if (marker && marker.parentNode === body) return;
    // Leptos may reuse the article element when a slug changes and replace only
    // its inner_html. A fresh first child means the new post needs normalization.
    delete body.dataset.markdownInit;
    // Tight Markdown lists contain text directly in <li>. Give each run of
    // inline children its own block without flattening nested lists.
    body.querySelectorAll("li").forEach((li) => {
      let block = null;
      for (const child of Array.from(li.childNodes)) {
        if (child.nodeType === Node.ELEMENT_NODE &&
            child.matches("p, ul, ol, blockquote, pre, h1, h2, h3, h4, h5, h6, figure, hr")) {
          block = null;
          continue;
        }
        if (!block) {
          block = document.createElement("div");
          block.className = "markdown-inline";
          li.insertBefore(block, child);
        }
        block.appendChild(child);
      }
    });

    // Split mixed image/text paragraphs with native DOM Ranges. Cloning a Range
    // retains nested emphasis and links on either side of the image.
    let image;
    while ((image = body.querySelector("img:not([data-blog-image])"))) {
      image.dataset.blogImage = "1";
      let figure = image.closest("figure");
      if (!figure) {
        figure = document.createElement("figure");
        const block = image.closest("p, h1, h2, h3, h4, h5, h6, .markdown-inline");
        const link = image.closest("a");
        const imageLink = link ? link.cloneNode(false) : null;
        if (block) {
          const before = document.createRange();
          before.selectNodeContents(block);
          before.setEndBefore(image);
          const after = document.createRange();
          after.selectNodeContents(block);
          after.setStartAfter(image);
          const fragments = [before.cloneContents(), after.cloneContents()];
          const nodes = fragments.map((fragment) => {
            if (!fragment.textContent.trim() && !fragment.querySelector("img")) return null;
            const node = block.cloneNode(false);
            node.removeAttribute("id");
            node.appendChild(fragment);
            return node;
          });
          if (imageLink) {
            imageLink.removeAttribute("id");
            imageLink.appendChild(image);
            figure.appendChild(imageLink);
          } else {
            figure.appendChild(image);
          }
          if (block.id) {
            (nodes.find(Boolean) || figure).id = block.id;
          }
          block.replaceWith(...[nodes[0], figure, nodes[1]].filter(Boolean));
        } else {
          image.replaceWith(figure);
          figure.appendChild(image);
        }
      }
      figure.classList.add("blog-image");
      const width = image.getAttribute("width");
      if (width && /^[1-9]\d*$/.test(width)) {
        figure.style.setProperty("--blog-image-width", width + "px");
      }
      // Capture this image, not the loop variable used for subsequent images.
      const currentImage = image;
      const onSize = () => {
        currentImage.style.aspectRatio = currentImage.naturalWidth > 0
          ? currentImage.naturalWidth + " / " + currentImage.naturalHeight
          : "4 / 3";
        scheduleAll();
      };
      onSize();
      image.addEventListener("load", onSize);
      image.addEventListener("error", onSize);
    }

    // Images start on their own row. Their group collapses to the remaining
    // prose when an image detaches; Pretext wraps around its new position.
    body.querySelectorAll(".blog-image").forEach((figure) => {
      const group = document.createElement("div");
      group.className = "markdown-image-group";
      figure.before(group);
      let sibling = figure.nextSibling;
      group.appendChild(figure);
      while (sibling && !(sibling.nodeType === Node.ELEMENT_NODE &&
             sibling.classList.contains("blog-image"))) {
        const next = sibling.nextSibling;
        group.appendChild(sibling);
        sibling = next;
      }
      flow.imageGroups.push({ group, figure });
      flow.imageObserver?.observe(figure);
      makeFloatingDraggable(figure);
    });
    body.dataset.markdownInit = "1";
    flow.markdownBodies.set(body, body.firstElementChild);
  });
}

async function copyCodeText(text) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch (_) {
      // Fall back when clipboard permissions are unavailable.
    }
  }
  // The WireGuard test site uses HTTP, where the modern Clipboard API is not
  // available. Keep this fallback inside the user's button click.
  const textarea = document.createElement("textarea");
  textarea.value = text || " ";
  textarea.readOnly = true;
  textarea.tabIndex = -1;
  textarea.setAttribute("aria-hidden", "true");
  textarea.style.cssText = "position:fixed;top:0;left:0;width:1px;height:1px;opacity:0;pointer-events:none";
  const focused = document.activeElement;
  const selection = window.getSelection();
  const ranges = selection ? Array.from({ length: selection.rangeCount },
    (_, i) => selection.getRangeAt(i).cloneRange()) : [];
  const onCopy = (event) => {
    if (event.clipboardData) {
      event.clipboardData.setData("text/plain", text);
      event.preventDefault();
    }
  };
  document.body.appendChild(textarea);
  document.addEventListener("copy", onCopy, true);
  try {
    textarea.focus({ preventScroll: true });
    textarea.select();
    if (!document.execCommand("copy")) throw new Error("Copy failed");
  } finally {
    document.removeEventListener("copy", onCopy, true);
    textarea.remove();
    if (focused?.isConnected) focused.focus({ preventScroll: true });
    if (selection) {
      selection.removeAllRanges();
      for (const range of ranges) {
        if (range.commonAncestorContainer.isConnected) selection.addRange(range);
      }
    }
  }
}

function addCodeCopyButton(el, source) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "code-copy-button";
  button.textContent = "Copy";
  button.setAttribute("aria-label", "Copy code");
  const status = document.createElement("span");
  status.className = "code-copy-status";
  status.setAttribute("role", "status");
  let copying = false;
  let resetTimer;
  button.addEventListener("click", async () => {
    if (copying) return;
    copying = true;
    clearTimeout(resetTimer);
    status.textContent = "";
    try {
      await copyCodeText((source.querySelector("code") || source).textContent);
      button.textContent = "Copied!";
      status.textContent = "Code copied.";
    } catch (_) {
      button.textContent = "Copy failed";
      status.textContent = "Could not copy. Select the code and copy it manually.";
    } finally {
      copying = false;
      resetTimer = setTimeout(() => {
        button.textContent = "Copy";
        status.textContent = "";
      }, 2000);
    }
  });
  el.classList.add("code-copy-enabled");
  el.append(button, status);
}

function createMarkdownInstance(el) {
  const api = flow.pretext;
  const isCode = el.tagName === "PRE";
  const source = document.createElement("span");
  source.className = "flow-source";
  source.append(...Array.from(el.childNodes));
  el.appendChild(source);
  el.style.position = "relative";
  el.dataset.flowInit = "1";
  if (isCode) addCodeCopyButton(el, source);
  const links = Array.from(source.querySelectorAll("a"));
  const pool = [];
  let chunks = [];
  let lineHeight;
  let box;
  let sourceLength;
  let lastKey = "";
  let lastWidth = 0;
  const renderedLines = new WeakMap();

  function refreshTypography() {
    const cs = getComputedStyle(el);
    box = {
      left: parseFloat(cs.paddingLeft) || 0,
      right: parseFloat(cs.paddingRight) || 0,
      top: parseFloat(cs.paddingTop) || 0,
      bottom: parseFloat(cs.paddingBottom) || 0,
      borderLeft: el.clientLeft,
      borderTop: el.clientTop,
    };
    sourceLength = source.textContent.length;
    lineHeight = resolveTypography(el).lineHeight;
    if (isCode) {
      const style = resolveTypography(source.querySelector("code") || el);
      const text = source.textContent.replace(/\n$/, "");
      // Keep server-generated token colors while measuring the whole code line
      // with Pretext. Token boundaries must not change wrapping or tab stops.
      const tokens = [];
      const walker = document.createTreeWalker(source, NodeFilter.SHOW_TEXT);
      let offset = 0;
      let node;
      while ((node = walker.nextNode())) {
        const end = offset + node.textContent.length;
        if (end > offset) tokens.push({ start: offset, end,
          color: getComputedStyle(node.parentElement).color });
        offset = end;
      }
      let start = 0;
      let tokenIndex = 0;
      chunks = text.split("\n").map((line) => {
        const end = start + line.length;
        while (tokenIndex < tokens.length && tokens[tokenIndex].end <= start) tokenIndex++;
        const colors = [];
        for (let i = tokenIndex; i < tokens.length && tokens[i].start < end; i++) {
          colors.push({ start: Math.max(start, tokens[i].start) - start,
            end: Math.min(end, tokens[i].end) - start, color: tokens[i].color });
        }
        const prepared = api.prepareWithSegments(line, style.font, { whiteSpace: "pre-wrap" });
        let position = 0;
        const segmentOffsets = prepared.segments.map((segment) => {
          const index = position;
          position += segment.length;
          return index;
        });
        segmentOffsets.push(position);
        start = end + 1;
        return { empty: line === "", font: style.font, prepared, colors,
          segmentOffsets, graphemeOffsets: new Map() };
      });
    } else {
      const groups = [[]];
      function visit(node, ancestors) {
        if (node.nodeType === Node.TEXT_NODE) {
          const typography = resolveTypography(node.parentElement);
          const cs = getComputedStyle(node.parentElement);
          lineHeight = Math.max(lineHeight, typography.lineHeight);
          groups[groups.length - 1].push({
            text: node.textContent,
            font: typography.font,
            letterSpacing: parseFloat(cs.letterSpacing) || 0,
            ancestors,
          });
        } else if (node.nodeType === Node.ELEMENT_NODE) {
          if (node.tagName === "BR") {
            groups.push([]);
          } else {
            for (const child of node.childNodes) visit(child, [...ancestors, node]);
          }
        }
      }
      for (const child of source.childNodes) visit(child, []);
      chunks = groups.map((items) => ({
        items,
        empty: !items.some((item) => item.text.trim()),
        prepared: api.prepareRichInline(items),
      }));
    }
    lastKey = "";
  }
  refreshTypography();

  function nextLine(chunk, cursor, width) {
    return isCode
      ? api.layoutNextLineRange(chunk.prepared, cursor, width)
      : api.layoutNextRichInlineLineRange(chunk.prepared, width, cursor);
  }

  function renderCodeFragments(line, node) {
    const chunk = line.chunk;
    const cursor = line.content.start;
    let start = chunk.segmentOffsets[cursor.segmentIndex];
    if (cursor.graphemeIndex > 0) {
      let offsets = chunk.graphemeOffsets.get(cursor.segmentIndex);
      if (!offsets) {
        const segment = chunk.prepared.segments[cursor.segmentIndex];
        offsets = Array.from(new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(segment),
          (part) => part.index);
        offsets.push(segment.length);
        chunk.graphemeOffsets.set(cursor.segmentIndex, offsets);
      }
      start += offsets[cursor.graphemeIndex];
    }
    const end = start + line.content.text.length;
    const fragment = document.createDocumentFragment();
    for (const token of chunk.colors) {
      const from = Math.max(start, token.start);
      const to = Math.min(end, token.end);
      if (from >= to) continue;
      const span = document.createElement("span");
      span.style.color = token.color;
      span.textContent = line.content.text.slice(from - start, to - start);
      fragment.appendChild(span);
    }
    node.replaceChildren(fragment);
  }

  function focusedLink() {
    return links.indexOf(document.activeElement);
  }

  function mirrorFocus() {
    const index = focusedLink();
    const fragments = el.querySelectorAll("[data-flow-link]");
    fragments.forEach((link) => link.classList.toggle("flow-link-focus",
      Number(link.dataset.flowLink) === index));
  }
  source.addEventListener("focusin", () => {
    mirrorFocus();
    const fragment = el.querySelector('[data-flow-link="' + focusedLink() + '"]');
    if (fragment) {
      const rect = fragment.getBoundingClientRect();
      if (rect.top < 0 || rect.bottom > window.innerHeight) {
        fragment.scrollIntoView({ block: "nearest" });
      }
    }
  });
  source.addEventListener("focusout", () => {
    el.querySelectorAll(".flow-link-focus").forEach((node) => node.classList.remove("flow-link-focus"));
  });

  function renderFragments(line, node) {
    const fragment = document.createDocumentFragment();
    for (const part of line.content.fragments) {
      const item = line.chunk.items[part.itemIndex];
      const run = document.createElement("span");
      if (part.gapBefore > 0) {
        const gap = document.createElement("span");
        gap.textContent = " ";
        gap.style.display = "inline-block";
        gap.style.width = part.gapBefore + "px";
        fragment.appendChild(gap);
      } else if (part.gapBefore < 0) {
        run.style.marginLeft = part.gapBefore + "px";
      }
      let parent = run;
      for (const ancestor of item.ancestors) {
        const clone = ancestor.cloneNode(false);
        clone.removeAttribute("id");
        if (clone.tagName === "A") {
          const index = links.indexOf(ancestor);
          clone.tabIndex = -1;
          clone.dataset.flowLink = String(index);
          clone.addEventListener("pointerdown", (event) => {
            // Keep keyboard/screen-reader focus on the one canonical link.
            event.preventDefault();
            links[index].focus({ preventScroll: true });
          });
        }
        parent.appendChild(clone);
        parent = clone;
      }
      // Pretext excludes hanging end-of-line spaces from fit width. CSS normal
      // whitespace suppresses them too; omit them from the visual fragment.
      parent.textContent = part === line.content.fragments.at(-1)
        ? part.text.replace(/[ \t\r\n]+$/, "") : part.text;
      fragment.appendChild(run);
    }
    node.replaceChildren(fragment);
  }

  function relayout() {
    const { left, right, top, bottom, borderLeft, borderTop } = box;
    const width = el.clientWidth - left - right;
    if (width <= 0) return;
    const obstacles = obstaclesRelTo(el, false, !lastKey || lastWidth !== width ? Infinity : 0)
      .map((obstacle) => obstacle.left !== undefined
      ? { left: obstacle.left - left - borderLeft, right: obstacle.right - left - borderLeft,
          top: obstacle.top - top - borderTop, bottom: obstacle.bottom - top - borderTop }
      : { cx: obstacle.cx - left - borderLeft, cy: obstacle.cy - top - borderTop,
          radius: obstacle.radius });
    const key = width + "|" + obstaclesKey(obstacles) + "|" + lineHeight;
    if (key === lastKey) return;
    const segmentsForY = buildSegmentsFn(width, obstacles, lineHeight, MIN_LINE_WIDTH);
    const lines = [];
    let y = 0;
    const obstacleBottom = Math.max(0, ...obstacles.map((o) =>
      o.bottom !== undefined ? o.bottom + PHOTO_MARGIN : o.cy + o.radius + PHOTO_MARGIN));
    // A source-based bound detects stalled layout without silently truncating
    // long posts; blocked bands can only persist through the finite obstacles.
    const maxRows = Math.ceil(obstacleBottom / lineHeight) + sourceLength + chunks.length + 1;
    let rows = 0;
    for (const chunk of chunks) {
      if (chunk.empty) {
        y += lineHeight;
        continue;
      }
      let cursor = isCode ? { segmentIndex: 0, graphemeIndex: 0 } : undefined;
      let done = false;
      while (!done) {
        if (++rows > maxRows) throw new Error("Markdown layout made no progress");
        for (const segment of segmentsForY(y)) {
          const range = nextLine(chunk, cursor, segment.w);
          if (!range) {
            done = true;
            break;
          }
          lines.push({ x: segment.x + left, y: y + top, range, chunk });
          cursor = range.end;
          if (!nextLine(chunk, cursor, Infinity)) {
            done = true;
            break; // A hard break advances a row, including two-gap rows.
          }
        }
        y += lineHeight;
      }
    }
    // Prepare every line before hiding the accessible, readable source.
    lines.forEach((line, index) => {
      let node = pool[index];
      if (!node) {
        node = document.createElement("span");
        node.className = "flow-line";
        node.setAttribute("aria-hidden", "true");
        node.style.position = "absolute";
        pool.push(node);
        el.appendChild(node);
      }
      const top = line.y + "px";
      const left = line.x + "px";
      const height = lineHeight + "px";
      if (node.style.display === "none") node.style.display = "";
      if (node.style.top !== top) node.style.top = top;
      if (node.style.left !== left) node.style.left = left;
      if (node.style.lineHeight !== height) node.style.lineHeight = height;
      // Moving a line does not change its markup. Rebuild fragments only when
      // its prepared text or its actual line-break range changes.
      const signature = JSON.stringify(line.range.fragments || [line.range.start, line.range.end]);
      const previous = renderedLines.get(node);
      if (!previous || previous.chunk !== line.chunk || previous.signature !== signature) {
        line.content = isCode
          ? api.materializeLineRange(line.chunk.prepared, line.range)
          : api.materializeRichInlineLineRange(line.chunk.prepared, line.range);
        if (isCode) {
          node.style.font = line.chunk.font;
          node.style.lineHeight = height;
          renderCodeFragments(line, node);
        } else {
          renderFragments(line, node);
        }
        renderedLines.set(node, { chunk: line.chunk, signature });
      }
    });
    for (let i = lines.length; i < pool.length; i++) {
      if (pool[i].style.display !== "none") pool[i].style.display = "none";
    }
    const height = y + top + bottom + borderTop * 2 + "px";
    if (el.style.height !== height) el.style.height = height;
    if (!el.classList.contains("markdown-flow-ready")) el.classList.add("markdown-flow-ready");
    if (focusedLink() !== -1) mirrorFocus();
    lastKey = key;
    lastWidth = width;
  }

  return { el, isConnected: () => el.isConnected && source.parentNode === el, relayout, refreshTypography };
}

// Reflow every instance, coalesced to one animation frame.
function scheduleAll() {
  if (flow.frame || flow.layingOut) return;
  flow.frame = requestAnimationFrame(() => {
    flow.frame = 0;
    flow.layingOut = true;
    try {
      const moved = pendingMoves.size > 0;
      for (const [element, { x, y }] of pendingMoves) {
        if (element.isConnected) element.style.transform = `translate(${x}px, ${y}px)`;
      }
      pendingMoves.clear();
      if (moved) document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
      // Image sizes are maintained by ResizeObserver, not re-read on every drag.
      flow.obstacleRects = new Map();
      for (const obstacle of flow.obstacleEls) {
        // Document coordinates stay valid if scroll anchoring changes scrollY
        // while earlier blocks reflow (especially on a viewport resize).
        if (obstacle.isConnected) flow.obstacleRects.set(obstacle, documentRect(obstacle));
      }
      for (const instance of flow.instances) {
        if (instance.isConnected()) instance.relayout();
      }
    } finally {
      flow.obstacleRects = null;
      flow.layingOut = false;
    }
  });
}

// Discover new flowable elements under `root` and drop instances whose elements
// have left the DOM (e.g. after a client-side route change). Idempotent.
function scan(root) {
  let added = 0;
  flow.imageGroups = flow.imageGroups.filter(({ group, figure }) => {
    if (group.isConnected) return true;
    flow.imageObserver?.unobserve(figure);
    return false;
  });
  prepareMarkdown(root);
  root.querySelectorAll(".markdown-body p, .markdown-body h1, .markdown-body h2, .markdown-body h3, .markdown-body h4, .markdown-body h5, .markdown-body h6, .markdown-body .markdown-inline, .markdown-body pre").forEach((el) => {
    if (el.dataset.flowInit === "1" || el.closest(".flow-source, .flow-line")) return;
    if ((!el.textContent.trim() && el.tagName !== "PRE") || el.querySelector("img")) return;
    flow.instances.push(createMarkdownInstance(el));
    added += 1;
  });
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
  // Earlier blocks can change the vertical position of later blocks. Layout
  // them in document order so each obstacle calculation sees the final offset.
  flow.instances.sort((a, b) => a.el.compareDocumentPosition(b.el) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1);
  // Keep the obstacle set current: certificate icons may have been added by a
  // route change (or dropped from the DOM).
  refreshObstacles();
  if (added > 0 || flow.imageGroups.length > 0) scheduleAll();
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
      flow.imageObserver = new ResizeObserver(scheduleAll);

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

      // Discover route/async content changes without scanning the whole article
      // again for our own visual line and copy-button updates.
      const mo = new MutationObserver((muts) => {
        for (let i = 0; i < muts.length; i++) {
          const mutation = muts[i];
          if (mutation.target.nodeType === Node.ELEMENT_NODE &&
              mutation.target.closest(".flow-line, .code-copy-button, .code-copy-status")) continue;
          const changed = [...mutation.addedNodes, ...mutation.removedNodes];
          if (changed.length && changed.every((node) => node.nodeType === Node.ELEMENT_NODE &&
              node.matches(".flow-line, .code-copy-button, .code-copy-status"))) continue;
          if (muts[i].addedNodes.length || muts[i].removedNodes.length) {
            scheduleScan(root);
            break;
          }
        }
      });
      mo.observe(root, { childList: true, subtree: true });

      if (document.fonts && document.fonts.ready) {
        document.fonts.ready.then(() => {
          pretext.clearCache();
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
