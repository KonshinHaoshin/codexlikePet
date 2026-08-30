import { CELL_HEIGHT, CELL_WIDTH, type LookDirection } from "./pet/atlas";
import { loadPet } from "./pet/loader";
import { PetEngine } from "./pet/engine";
import { watchCursorDirection } from "./pet/cursorWatcher";
import { attachDrag, attachGestures, type Gesture } from "./pet/window";

const PET_BASE = import.meta.env.BASE_URL + "pets/sakimiao";
const SCALE = 1; // 1 = 原大小 192x208;调大可放大桌宠

async function boot(): Promise<void> {
  const { canvas: atlas } = await loadPet(PET_BASE);

  const stage = document.querySelector<HTMLCanvasElement>("#stage")!;
  stage.width = CELL_WIDTH * SCALE;
  stage.height = CELL_HEIGHT * SCALE;
  const engine = new PetEngine(atlas, stage, SCALE);
  engine.play(true);

  // Cursor chasing. Look frames have a content deadzone; a null means the
  // cursor is near the pet, so fall back to the idle loop.
  let lastDirection: LookDirection | null = null;
  watchCursorDirection((d) => {
    lastDirection = d === null ? null : (d as LookDirection);
    engine.setLook(lastDirection);
  });

  const gestureToState: Record<Gesture, () => void> = {
    left: () => engine.setState("jumping"),
    right: () => engine.setState("failed"),
  };

  const petEl = document.querySelector<HTMLElement>("#pet")!;
  attachDrag(petEl, (enabled) => {
    if (enabled) {
      engine.setLook(lastDirection);
    } else {
      engine.setLook(null);
      engine.setState("running");
    }
  });
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