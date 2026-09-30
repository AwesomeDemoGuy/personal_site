// Temporary pointer-drag bridge. Rust owns text preparation and layout.
// Component owners dispose listeners; Leptos observes navigation.
const PHOTO_MOVE_EVENT = "photomove";

// ---------------------------------------------------------------------------
// Draggable reset registry + navigation hook
// ---------------------------------------------------------------------------

// Every draggable registers a { el, reset } here. On a client-side navigation
// we restore each draggable to its default position (and discard any popped-out,
// now-orphaned floating element), so a new page never inherits the previous
// page's dragged-around layout.
const dragResets = [];

function registerDragReset(el, reset, dispose) {
  dragResets.push({ el, reset, dispose });
}

export function resetAllDraggables() {
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
      dragResets[i].dispose?.();
      dragResets.splice(i, 1);
    }
  }
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
  let pointerId = null;

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
    pointerId = e.pointerId;
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
    dragging = false;
    element.classList.remove("dragging");
    if (pointerId !== null && element.hasPointerCapture(pointerId)) element.releasePointerCapture(pointerId);
    offsetX = 0;
    offsetY = 0;
    element.style.transform = "";
    document.dispatchEvent(new CustomEvent(PHOTO_MOVE_EVENT));
  }, () => {
    element.removeEventListener("pointerdown", onPointerDown);
    element.removeEventListener("pointermove", onPointerMove);
    element.removeEventListener("pointerup", onPointerUp);
    element.removeEventListener("pointercancel", onPointerUp);
  });
}

export function disposeDraggable(element) {
  const index = dragResets.findIndex(entry => entry.el === element);
  if (index < 0) return;
  const [entry] = dragResets.splice(index, 1);
  entry.reset();
  entry.dispose?.();
  delete element.dataset.draggableInit;
}
