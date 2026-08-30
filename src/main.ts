import { CELL_HEIGHT, CELL_WIDTH, type LookDirection } from "./pet/atlas";
import { loadPet } from "./pet/loader";
import { PetEngine } from "./pet/engine";
import { watchCursorDirection } from "./pet/cursorWatcher";
import { attachDrag, attachGestures, dragState, type DragDirection, type Gesture } from "./pet/window";

const PET_BASE = import.meta.env.BASE_URL + "pets/sakimiao";
const SCALE = 1; // 1 = 原大小 192x208;调大可放大桌宠

async function boot(): Promise<void> {
  const { canvas: atlas } = await loadPet(PET_BASE);

  const stage = document.querySelector<HTMLCanvasElement>("#stage")!;
  stage.width = CELL_WIDTH * SCALE;
  stage.height = CELL_HEIGHT * SCALE;
  const engine = new PetEngine(atlas, stage, SCALE);
  engine.play(true);

  // Cursor chasing is limited to the pet window and a small surrounding area.
  // A null means the cursor is outside that area or inside the deadzone.
  let lastDirection: LookDirection | null = null;
  let dragging = false;
  watchCursorDirection((d) => {
    lastDirection = d === null ? null : (d as LookDirection);
    if (!dragging) engine.setLook(lastDirection);
  });

  const petEl = document.querySelector<HTMLElement>("#pet")!;

  // Drag moves the frameless window; freeze look-chasing while dragging and
  // resume the nearby cursor direction after release.
  attachDrag(petEl, (enabled, direction: DragDirection | null) => {
    dragging = enabled;
    if (enabled) {
      engine.setLook(null);
      if (direction === "left") {
        engine.setState("running-left");
      } else if (direction === "right") {
        engine.setState("running-right");
      } else {
        engine.setState("running");
      }
    } else {
      engine.setLook(lastDirection);
      engine.setState("idle");
    }
  });

  // Hover: trigger a one-shot jumping gesture when the mouse enters the pet.
  // `playOnce` animates through the row then settles back to idle; the look
  // tracking resumes once the gesture loop completes.
  let hovered = false;
  petEl.addEventListener("pointerenter", () => {
    if (hovered || dragState.current) return;
    hovered = true;
    engine.setLook(null);
    engine.playOnce("jumping");
  });
  petEl.addEventListener("pointerleave", () => {
    hovered = false;
    if (!dragState.current) {
      engine.setLook(lastDirection);
      engine.setState("idle");
    }
  });

  const gestureToState: Record<Gesture, () => void> = {
    left: () => engine.playOnce("jumping"),
    right: () => engine.playOnce("failed"),
  };
  attachGestures(petEl, (g) => gestureToState[g]());
}

boot().catch((err) => {
  console.error("failed to boot pet:", err);
  const stage = document.querySelector<HTMLCanvasElement>("#stage")!;
  const ctx = stage.getContext("2d")!;
  ctx.fillStyle = "#333";
  ctx.fillRect(0, 0, stage.width, stage.height);
  ctx.fillStyle = "#fff";
  ctx.font = "13px monospace";
  ctx.fillText(String(err?.message ?? err), 10, 30);
});
