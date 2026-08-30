import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CELL_HEIGHT, CELL_WIDTH, type LookDirection } from "./pet/atlas";
import { loadPet } from "./pet/loader";
import { loadPetFromData } from "./pet/loader";
import { PetEngine } from "./pet/engine";
import { watchCursorDirection } from "./pet/cursorWatcher";
import { PetStateMachine, type PetAction } from "./pet/stateMachine";
import { attachDrag, attachGestures, dragState, type DragDirection, type Gesture } from "./pet/window";
import { PetWalker } from "./pet/walker";
import type { PetDialogue, PetSettings, PetSettingsEvent, RuntimeConfig } from "./pet/config";
import { DEFAULT_DIALOGUE, loadDialogue } from "./pet/dialogue";

const PETS_BASE = import.meta.env.BASE_URL + "pets";
type DialogueTrigger = Exclude<keyof PetDialogue, "version">;
const IDLE_SPEECH_DELAY_MS = 90_000;

async function boot(): Promise<void> {
  const window = getCurrentWindow();
  const runtime = await invoke<RuntimeConfig>("get_runtime_config", { windowLabel: window.label });
  const loadRuntimePet = async (): Promise<Awaited<ReturnType<typeof loadPet>>> => {
    if (runtime.source === "imported" && runtime.manifest && runtime.spritesheetDataUrl) {
      return loadPetFromData(runtime.manifest, runtime.spritesheetDataUrl);
    }
    if (!runtime.path) throw new Error(`宠物资源不存在：${runtime.petId}`);
    return loadPet(`${PETS_BASE}/${runtime.path}`);
  };

  const initialPet = await loadRuntimePet();
  let dialogue = runtime.dialogue ?? DEFAULT_DIALOGUE;
  if (runtime.source === "bundled" && runtime.path) {
    dialogue = await loadDialogue(`${PETS_BASE}/${runtime.path}`);
  }
  let settings: PetSettings = runtime.settings;
  const stage = document.querySelector<HTMLCanvasElement>("#stage")!;
  const petEl = document.querySelector<HTMLElement>("#pet")!;
  const speech = document.querySelector<HTMLElement>("#speech")!;
  const speechText = document.querySelector<HTMLElement>("#speech-text")!;
  const setStageSize = (scale: number): void => {
    stage.width = Math.round(CELL_WIDTH * scale);
    stage.height = Math.round(CELL_HEIGHT * scale);
  };
  setStageSize(settings.scale);
  petEl.style.opacity = String(settings.opacity);

  const engine = new PetEngine(initialPet.canvas, stage, settings.scale);
  const stateMachine = new PetStateMachine();
  let paused = settings.paused;
  let dragging = false;
  let hovered = false;
  let lastDirection: LookDirection | null = null;
  let walking = false;
  const dialogueIndices: Record<DialogueTrigger, number> = {
    doubleClick: 0,
    click: 0,
    rightClick: 0,
    walk: 0,
    drag: 0,
    idle: 0,
  };
  let speechTimer: number | undefined;
  let idleSpeechTimer: number | undefined;
  let clickTimer: number | undefined;
  let dragDialogueShown = false;

  const sayLine = (trigger: DialogueTrigger): void => {
    if (paused || settings.quietMode) return;
    const lines = dialogue[trigger];
    if (!lines.length) return;
    const index = dialogueIndices[trigger] % lines.length;
    speechText.textContent = lines[index];
    dialogueIndices[trigger] += 1;
    speech.hidden = false;
    speech.classList.add("speech-visible");
    if (speechTimer !== undefined) globalThis.clearTimeout(speechTimer);
    speechTimer = globalThis.setTimeout(() => {
      speech.classList.remove("speech-visible");
      speechTimer = globalThis.setTimeout(() => {
        speech.hidden = true;
      }, 180);
    }, 3600);
  };

  const scheduleIdleSpeech = (): void => {
    if (idleSpeechTimer !== undefined) globalThis.clearTimeout(idleSpeechTimer);
    idleSpeechTimer = undefined;
    if (paused || settings.quietMode || !dialogue.idle.length) return;
    idleSpeechTimer = globalThis.setTimeout(() => {
      idleSpeechTimer = undefined;
      if (!paused && !settings.quietMode && !dragging && !walking && !stateMachine.hasAction()) {
        sayLine("idle");
      }
      scheduleIdleSpeech();
    }, IDLE_SPEECH_DELAY_MS);
  };

  const walker = new PetWalker((isWalking, direction) => {
    if (isWalking && dragging) return;
    const startedWalking = isWalking && !walking;
    walking = isWalking;
    stateMachine.setWalking(isWalking, direction);
    if (isWalking) {
      engine.setLook(null);
      if (startedWalking) sayLine("walk");
    } else if (!dragging) engine.setLook(lastDirection);
    syncAnimation();
    if (!isWalking) void savePosition();
  });

  const syncAnimation = (): void => {
    engine.setState(stateMachine.animationState());
  };

  const savePosition = async (): Promise<void> => {
    try {
      const [position, scaleFactor] = await Promise.all([window.outerPosition(), window.scaleFactor()]);
      const logicalPosition = position.toLogical(scaleFactor);
      await invoke("save_pet_position", {
        instanceId: runtime.instanceId,
        x: logicalPosition.x,
        y: logicalPosition.y,
      });
    } catch (error) {
      console.warn("failed to save pet position:", error);
    }
  };

  const applySettings = (next: PetSettings): void => {
    settings = next;
    paused = next.paused;
    setStageSize(next.scale);
    petEl.style.opacity = String(next.opacity);
    engine.setScale(next.scale);
    walker.setSettings(next.speed, next.wanderEnabled, next.quietMode);
    engine.play(!next.paused);
    if (next.paused || !next.wanderEnabled || next.quietMode) walker.stop();
    else if (!dragging) walker.start();
    if (!dragging) engine.setLook(lastDirection);
    syncAnimation();
    scheduleIdleSpeech();
  };

  const openPetManager = async (): Promise<void> => {
    try {
      await invoke("open_pet_manager");
    } catch (error) {
      console.error("failed to open pet manager:", error);
    }
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

  watchCursorDirection((direction) => {
    lastDirection = direction === null ? null : (direction as LookDirection);
    if (!dragging) engine.setLook(lastDirection);
  });

  attachDrag(
    petEl,
    (enabled, direction: DragDirection | null) => {
      dragging = enabled;
      if (enabled) walker.stop();
      if (enabled && stateMachine.hasAction()) {
        stateMachine.finishAction();
        engine.cancelAction();
      }
      stateMachine.setDragging(enabled, direction);
      if (enabled && direction && !dragDialogueShown) {
        dragDialogueShown = true;
        sayLine("drag");
      }
      if (enabled) engine.setLook(null);
      else engine.setLook(lastDirection);
      syncAnimation();
      if (enabled) void savePosition();
      if (!enabled) {
        dragDialogueShown = false;
        void savePosition();
        if (!paused && settings.wanderEnabled && !settings.quietMode) walker.start();
      }
    },
    () => !settings.lockPosition && !settings.clickThrough && !paused,
  );

  petEl.addEventListener("pointerenter", () => {
    if (hovered || dragState.current) return;
    hovered = true;
    playAction("jumping");
  });
  petEl.addEventListener("pointerleave", () => {
    hovered = false;
    if (!dragging && !stateMachine.hasAction()) engine.setLook(lastDirection);
  });
  petEl.addEventListener("dblclick", (event) => {
    event.preventDefault();
    if (clickTimer !== undefined) {
      globalThis.clearTimeout(clickTimer);
      clickTimer = undefined;
    }
    sayLine("doubleClick");
    playAction("jumping");
  });

  attachGestures(petEl, (gesture: Gesture) => {
    if (gesture === "right") {
      sayLine("rightClick");
      playAction("failed");
      return;
    }
    if (clickTimer !== undefined) globalThis.clearTimeout(clickTimer);
    clickTimer = globalThis.setTimeout(() => {
      clickTimer = undefined;
      sayLine("click");
      playAction("jumping");
    }, 320);
  });

  await listen<PetSettingsEvent>("pet://settings", ({ payload }) => {
    if (payload.petId === runtime.petId) applySettings(payload.settings);
  });
  await listen<string>("pet://command", ({ payload }) => {
    if (payload === "open-manager") {
      void openPetManager();
    }
  });

  document.title = initialPet.manifest.displayName;
  engine.play(!paused);
  walker.setSettings(settings.speed, settings.wanderEnabled, settings.quietMode);
  scheduleIdleSpeech();
  if (!paused && settings.wanderEnabled && !settings.quietMode) walker.start();
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
