import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";

/**
 * Shared drag-state flag. Set to true while the window is being dragged so
 * hover logic and look-chasing can coordinate.
 */
export const dragState = { current: false };

/**
 * Makes the frameless transparent pet window draggable from anywhere inside
 * the window, using pointer events + the Tauri window API to reposition it.
 */
export function attachDrag(
  element: HTMLElement,
  onDragChange?: (dragging: boolean) => void,
): void {
  const win = getCurrentWindow();
  let dragging = false;
  let startPointer = { x: 0, y: 0 };
  let startWin: { x: number; y: number } | null = null;
  let moved = false;

  element.addEventListener("pointerdown", async (e) => {
    if (e.button !== 0) return;
    dragging = true;
    moved = false;
    startPointer = { x: e.screenX, y: e.screenY };
    try {
      const pos = await win.outerPosition();
      startWin = { x: pos.x, y: pos.y };
    } catch {
      startWin = null;
    }
    dragState.current = true;
    onDragChange?.(true);
    element.setPointerCapture(e.pointerId);
  });

  element.addEventListener("pointermove", async (e) => {
    if (!dragging || !startWin) return;
    const dx = e.screenX - startPointer.x;
    const dy = e.screenY - startPointer.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) moved = true;
    if (moved) {
      await win.setPosition(new PhysicalPosition(startWin.x + dx, startWin.y + dy));
    }
  });

  const endDrag = (e: PointerEvent) => {
    if (!dragging) return;
    dragging = false;
    dragState.current = false;
    onDragChange?.(false);
    try {
      element.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
  };

  element.addEventListener("pointerup", endDrag);
  element.addEventListener("pointercancel", endDrag);
}

export type Gesture = "left" | "right";

/**
 * Fires simple gestures (single left-click, right-click) on the pet window so
 * the engine can react. Purely additive: does not interfere with dragging.
 */
export function attachGestures(
  element: HTMLElement,
  onGesture: (g: Gesture) => void,
): void {
  let downAt = 0;
  let lastPointer = { x: 0, y: 0 };

  element.addEventListener("pointerdown", (e) => {
    downAt = performance.now();
    lastPointer = { x: e.screenX, y: e.screenY };
  });

  element.addEventListener("pointerup", (e) => {
    const dt = performance.now() - downAt;
    const moved =
      Math.abs(e.screenX - lastPointer.x) + Math.abs(e.screenY - lastPointer.y) > 8;
    if (moved || dt > 500) return; // treat as drag / hold, not a click
    onGesture(e.button === 2 ? "right" : "left");
  });

  element.addEventListener("contextmenu", (e) => e.preventDefault());
}