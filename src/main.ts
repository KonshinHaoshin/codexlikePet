import { listen } from "@tauri-apps/api/event";
import { CELL_HEIGHT, CELL_WIDTH, type LookDirection } from "./pet/atlas";
import { loadPetCatalog, type PetCatalogEntry } from "./pet/catalog";
import { loadPet } from "./pet/loader";
import { PetEngine } from "./pet/engine";
import { watchCursorDirection } from "./pet/cursorWatcher";
import { PetStateMachine, type PetAction } from "./pet/stateMachine";
import { attachDrag, attachGestures, dragState, type DragDirection, type Gesture } from "./pet/window";
import { PetWalker } from "./pet/walker";

const PETS_BASE = import.meta.env.BASE_URL + "pets";
const DEFAULT_PET_ID = "sakimiao";
const SCALE = 1; // 1 = 原大小 192x208; 调大可放大桌宠

async function boot(): Promise<void> {
  const catalog = await loadPetCatalog(PETS_BASE);
  const firstPet = catalog.find((pet) => pet.id === DEFAULT_PET_ID) ?? catalog[0];
  const initialPet = await loadPet(`${PETS_BASE}/${firstPet.path}`);

  const stage = document.querySelector<HTMLCanvasElement>("#stage")!;
  stage.width = CELL_WIDTH * SCALE;
  stage.height = CELL_HEIGHT * SCALE;
  const engine = new PetEngine(initialPet.canvas, stage, SCALE);
  const stateMachine = new PetStateMachine();
  engine.play(true);

  let activePetId = firstPet.id;
  let lastDirection: LookDirection | null = null;
  let dragging = false;
  let hovered = false;
  let paused = false;
  let switchToken = 0;

  const syncAnimation = (): void => {
    engine.setState(stateMachine.animationState());
  };

  const playAction = (action: PetAction): void => {
    if (paused || dragging || !stateMachine.startAction(action)) return;
    engine.setLook(null);
    engine.playOnce(action, () => {
      stateMachine.finishAction();
      if (!dragging) engine.setLook(lastDirection);
      syncAnimation();
    });
  };

  // Cursor chasing is limited to the pet window and a small surrounding area.
  // A null means the cursor is outside that area or inside the deadzone.
  watchCursorDirection((direction) => {
    lastDirection = direction === null ? null : (direction as LookDirection);
    if (!dragging) engine.setLook(lastDirection);
  });

  const petEl = document.querySelector<HTMLElement>("#pet")!;

  const walker = new PetWalker((walking, direction) => {
    // A pointer drag always owns the window. Ignore a stale walk-start event
    // if it races with the user's pointerdown.
    if (walking && dragging) return;
    stateMachine.setWalking(walking, direction);
    if (walking) engine.setLook(null);
    else if (!dragging) engine.setLook(lastDirection);
    syncAnimation();
  });

  // Drag moves the frameless window. The state machine turns the drag vector
  // into running-left/running-right and keeps the generic running state before
  // a horizontal direction has been established.
  attachDrag(petEl, (enabled, direction: DragDirection | null) => {
    dragging = enabled;
    if (enabled) walker.stop();
    if (enabled && stateMachine.hasAction()) {
      stateMachine.finishAction();
      engine.cancelAction();
    }
    stateMachine.setDragging(enabled, direction);
    if (enabled) engine.setLook(null);
    else engine.setLook(lastDirection);
    syncAnimation();
    if (!enabled && !paused) walker.start();
  });

  // Hovering the pet makes it react once, while the cursor remains local to the
  // pet window. Leaving it restores the nearby look pose when no action runs.
  petEl.addEventListener("pointerenter", () => {
    if (hovered || dragState.current) return;
    hovered = true;
    playAction("jumping");
  });
  petEl.addEventListener("pointerleave", () => {
    hovered = false;
    if (!dragging && !stateMachine.hasAction()) engine.setLook(lastDirection);
  });

  const gestureToAction: Record<Gesture, PetAction> = {
    left: "jumping",
    right: "failed",
  };
  attachGestures(petEl, (gesture) => playAction(gestureToAction[gesture]));

  const switchPet = async (id: string): Promise<void> => {
    const nextPet: PetCatalogEntry | undefined = catalog.find((pet) => pet.id === id);
    if (!nextPet || nextPet.id === activePetId) return;

    const token = ++switchToken;
    walker.stop();
    try {
      const loaded = await loadPet(`${PETS_BASE}/${nextPet.path}`);
      if (token !== switchToken) return;
      activePetId = nextPet.id;
      engine.cancelAction();
      stateMachine.reset();
      engine.setSource(loaded.canvas);
      engine.setLook(lastDirection);
      syncAnimation();
      document.title = loaded.manifest.displayName;
    } catch (error) {
      console.error(`failed to switch pet to ${id}:`, error);
    } finally {
      if (!paused && !dragging) walker.start();
    }
  };

  await listen<string>("pet://command", ({ payload }) => {
    if (payload === "toggle-pause") {
      paused = !paused;
      if (paused) walker.stop();
      else if (!dragging) walker.start();
      engine.play(!paused);
      return;
    }
    if (payload.startsWith("select:")) void switchPet(payload.slice("select:".length));
  });

  // Keep long idle periods and only occasionally let the pet walk.
  walker.start();
  document.title = initialPet.manifest.displayName;
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
