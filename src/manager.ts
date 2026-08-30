import { invoke } from "@tauri-apps/api/core";
import { CELL_HEIGHT, CELL_WIDTH, type PetManifest } from "./pet/atlas";
import { loadPetCatalog, type PetCatalogEntry } from "./pet/catalog";

const PETS_BASE = import.meta.env.BASE_URL + "pets";
const PREVIEW_WIDTH = 96;
const PREVIEW_HEIGHT = 104;

interface LoadedPet {
  entry: PetCatalogEntry;
  manifest: PetManifest | null;
  preview: HTMLCanvasElement | null;
  error?: string;
}

let pets: LoadedPet[] = [];
let visiblePetIds = new Set<string>();

const list = document.querySelector<HTMLElement>("#pet-list")!;
const status = document.querySelector<HTMLElement>("#status")!;
const refreshButton = document.querySelector<HTMLButtonElement>("#refresh")!;

function setStatus(message: string, kind: "normal" | "error" = "normal"): void {
  status.textContent = message;
  status.dataset.kind = kind;
}

async function loadManifest(entry: PetCatalogEntry): Promise<PetManifest> {
  const response = await fetch(`${PETS_BASE}/${entry.path}/pet.json`);
  if (!response.ok) throw new Error(`pet.json 请求失败：${response.status}`);
  const manifest = (await response.json()) as Partial<PetManifest>;
  if (
    manifest.id !== entry.id ||
    typeof manifest.displayName !== "string" ||
    typeof manifest.description !== "string" ||
    manifest.spriteVersionNumber !== 2 ||
    typeof manifest.spritesheetPath !== "string"
  ) {
    throw new Error("不是有效的 V2 宠物资源");
  }
  return manifest as PetManifest;
}

function loadPreview(entry: PetCatalogEntry, manifest: PetManifest): Promise<HTMLCanvasElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    const canvas = document.createElement("canvas");
    canvas.width = PREVIEW_WIDTH;
    canvas.height = PREVIEW_HEIGHT;
    image.decoding = "async";
    image.onload = () => {
      const context = canvas.getContext("2d");
      if (!context) {
        reject(new Error("无法创建预览画布"));
        return;
      }
      context.imageSmoothingEnabled = true;
      context.clearRect(0, 0, PREVIEW_WIDTH, PREVIEW_HEIGHT);
      context.drawImage(
        image,
        0,
        0,
        CELL_WIDTH,
        CELL_HEIGHT,
        0,
        0,
        PREVIEW_WIDTH,
        PREVIEW_HEIGHT,
      );
      resolve(canvas);
    };
    image.onerror = () => reject(new Error(`无法读取 ${entry.id} 的预览`));
    image.src = `${PETS_BASE}/${entry.path}/${manifest.spritesheetPath}`;
  });
}

function createPreview(pet: LoadedPet): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.className = "pet-preview";
  if (pet.preview) {
    wrapper.append(pet.preview);
  } else {
    wrapper.textContent = "暂无预览";
    wrapper.classList.add("pet-preview-fallback");
  }
  return wrapper;
}

function render(): void {
  list.replaceChildren();

  for (const pet of pets) {
    const card = document.createElement("article");
    card.className = "pet-card";
    const isVisible = visiblePetIds.has(pet.entry.id);
    if (isVisible) card.classList.add("active");

    const preview = createPreview(pet);
    const content = document.createElement("div");
    content.className = "pet-content";

    const title = document.createElement("h2");
    title.textContent = pet.manifest?.displayName ?? pet.entry.id;
    content.append(title);

    const id = document.createElement("p");
    id.className = "pet-id";
    id.textContent = pet.entry.id;
    content.append(id);

    const description = document.createElement("p");
    description.className = "pet-description";
    description.textContent = pet.manifest?.description ?? pet.error ?? "资源信息不可用";
    content.append(description);

    const action = document.createElement("button");
    action.type = "button";
    action.className = isVisible ? "active-button" : "primary-button";
    action.textContent = isVisible ? "隐藏这只宠物" : "显示这只宠物";
    action.setAttribute("aria-pressed", String(isVisible));
    action.disabled = pet.manifest === null;
    action.addEventListener("click", async () => {
      action.disabled = true;
      try {
        const nextVisiblePetIds = await invoke<string[]>("set_pet_visible", {
          petId: pet.entry.id,
          visible: !isVisible,
        });
        visiblePetIds = new Set(nextVisiblePetIds);
        render();
        setStatus(`当前显示 ${visiblePetIds.size} 只宠物`);
      } catch (error) {
        action.disabled = false;
        setStatus(error instanceof Error ? error.message : String(error), "error");
      }
    });
    content.append(action);

    card.append(preview, content);
    list.append(card);
  }
}

async function reloadPets(): Promise<void> {
  refreshButton.disabled = true;
  setStatus("正在读取宠物列表…");
  try {
    const catalog = await loadPetCatalog(PETS_BASE);
    pets = await Promise.all(
      catalog.map(async (entry): Promise<LoadedPet> => {
        try {
          const manifest = await loadManifest(entry);
          const preview = await loadPreview(entry, manifest);
          return { entry, manifest, preview };
        } catch (error) {
          return {
            entry,
            manifest: null,
            preview: null,
            error: error instanceof Error ? error.message : String(error),
          };
        }
      }),
    );
    render();
    const availableCount = pets.filter((pet) => pet.manifest !== null).length;
    setStatus(`已安装 ${availableCount} 只宠物 · 当前显示 ${visiblePetIds.size} 只`);
  } catch (error) {
    pets = [];
    list.replaceChildren();
    setStatus(error instanceof Error ? error.message : String(error), "error");
  } finally {
    refreshButton.disabled = false;
  }
}

refreshButton.addEventListener("click", () => void reloadPets());

async function boot(): Promise<void> {
  try {
    visiblePetIds = new Set(await invoke<string[]>("get_visible_pets"));
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error), "error");
  }
  await reloadPets();
}

void boot().catch((error) => setStatus(error instanceof Error ? error.message : String(error), "error"));
