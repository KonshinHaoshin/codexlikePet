import { LogicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Shared drag-state flag. Set to true while the window is being dragged so
 * hover logic and look-chasing can coordinate.
 */
export const dragState = { current: false };

export type MoveDirection =
  | "left"
  | "right"
  | "up"
  | "down"
  | "up-left"
  | "up-right"
  | "down-left"
  | "down-right";
export type DragDirection = MoveDirection;

/**
 * Makes the frameless transparent pet window draggable from anywhere inside
 * the window, using pointer events + the Tauri window API to reposition it.
 */
export function attachDrag(
  element: HTMLElement,
  onDragChange?: (dragging: boolean, direction: DragDirection | null) => void,
  canDrag: () => boolean = () => true,
): void {
  const win = getCurrentWindow();
  let dragging = false;
  let dragToken = 0;
  let startPointer = { x: 0, y: 0 };
  let latestPointer = { x: 0, y: 0 };
  let startWin: { x: number; y: number } | null = null;
  let moved = false;
  let dragDirection: DragDirection | null = null;
  let pendingPosition: { x: number; y: number } | null = null;
  let flushingPosition = false;

  const flushPosition = async (): Promise<void> => {
    if (flushingPosition) return;
    flushingPosition = true;
    try {
      while (dragging && pendingPosition) {
        const next = pendingPosition;
        pendingPosition = null;
        try {
          await win.setPosition(new LogicalPosition(next.x, next.y));
        } catch {
          // The window may disappear while the pointer is still captured.
        }
      }
    } finally {
      flushingPosition = false;
    }
  };

  const updateFromPointer = (x: number, y: number): void => {
    if (!dragging || !startWin) return;
    const dx = x - startPointer.x;
    const dy = y - startPointer.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) moved = true;
    if (!moved) return;

    pendingPosition = { x: startWin.x + dx, y: startWin.y + dy };
    const horizontalDistance = Math.abs(dx);
    const verticalDistance = Math.abs(dy);
    const diagonal = horizontalDistance > 8 && verticalDistance > 8;
    const nextDirection: DragDirection = diagonal
      ? dy < 0
        ? dx < 0
          ? "up-left"
          : "up-right"
        : dx < 0
          ? "down-left"
          : "down-right"
      : horizontalDistance >= verticalDistance
        ? dx < 0
          ? "left"
          : "right"
        : dy < 0
          ? "up"
          : "down";
    if (dragDirection !== nextDirection) {
      dragDirection = nextDirection;
      onDragChange?.(true, nextDirection);
    }
    void flushPosition();
  };

  element.addEventListener("pointerdown", async (e) => {
    if (e.button !== 0 || !canDrag()) return;
    const token = ++dragToken;
    dragging = true;
    moved = false;
    startWin = null;
    pendingPosition = null;
    startPointer = { x: e.screenX, y: e.screenY };
    latestPointer = startPointer;
    dragDirection = null;
    dragState.current = true;
    onDragChange?.(true, null);
    element.setPointerCapture(e.pointerId);

    try {
      const [pos, scaleFactor] = await Promise.all([win.outerPosition(), win.scaleFactor()]);
      if (!dragging || token !== dragToken) return;
      const logicalPos = pos.toLogical(scaleFactor);
      startWin = { x: logicalPos.x, y: logicalPos.y };
      updateFromPointer(latestPointer.x, latestPointer.y);
    } catch {
      if (token !== dragToken) return;
      dragging = false;
      dragState.current = false;
      onDragChange?.(false, null);
      try {
        element.releasePointerCapture(e.pointerId);
      } catch {
        /* ignore */
      }
    }
  });

  element.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    latestPointer = { x: e.screenX, y: e.screenY };
    updateFromPointer(latestPointer.x, latestPointer.y);
  });

  const endDrag = (e: PointerEvent) => {
    if (!dragging) return;
    dragging = false;
    dragToken += 1;
    pendingPosition = null;
    startWin = null;
    dragDirection = null;
    dragState.current = false;
    onDragChange?.(false, null);
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
