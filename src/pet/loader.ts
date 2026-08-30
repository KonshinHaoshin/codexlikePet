import {
  CELL_HEIGHT,
  CELL_WIDTH,
  COLS,
  ROWS,
  STATE_TIMING,
  type AnimationState,
  type LookDirection,
  type PetManifest,
  lookSlot,
} from "./atlas";

export interface PetLoaderResult {
  manifest: PetManifest;
  canvas: HTMLCanvasElement;
}

async function decodePet(manifest: PetManifest, blob: Blob): Promise<PetLoaderResult> {
  if (manifest.spriteVersionNumber !== 2) {
    throw new Error(`unsupported spriteVersionNumber ${manifest.spriteVersionNumber}; only v2 is supported`);
  }

  const bitmap = await createImageBitmap(blob);
  const width = bitmap.width;
  const height = bitmap.height;
  if (width !== CELL_WIDTH * COLS || height !== CELL_HEIGHT * ROWS) {
    bitmap.close();
    throw new Error(`spritesheet is ${width}x${height}; expected ${CELL_WIDTH * COLS}x${CELL_HEIGHT * ROWS}`);
  }

  // Normalize the bitmap for drawImage + sub-rect slicing inside a canvas.
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  return { manifest, canvas };
}

// Fetch + decode a v2 pet package (pet.json + spritesheet) into a source canvas.
export async function loadPet(baseUrl: string): Promise<PetLoaderResult> {
  const manifestRes = await fetch(`${baseUrl}/pet.json`);
  if (!manifestRes.ok) throw new Error(`cannot fetch pet.json: ${manifestRes.status}`);
  const manifest: PetManifest = await manifestRes.json();
  const spritesheetRes = await fetch(`${baseUrl}/${manifest.spritesheetPath}`);
  if (!spritesheetRes.ok) throw new Error(`cannot fetch spritesheet: ${spritesheetRes.status}`);
  return decodePet(manifest, await spritesheetRes.blob());
}

/** Decode an imported pet whose spritesheet is delivered by the Rust backend. */
export async function loadPetFromData(
  manifest: PetManifest,
  spritesheetDataUrl: string,
): Promise<PetLoaderResult> {
  const response = await fetch(spritesheetDataUrl);
  return decodePet(manifest, await response.blob());
}

// Draw the plane sprite for a standard animation row onto a target canvas.
export function drawStateFrame(
  source: HTMLCanvasElement,
  target: CanvasRenderingContext2D,
  state: AnimationState,
  frame: number,
  scale: number,
  offsetX: number,
  offsetY: number,
): void {
  const spec = STATE_TIMING[state];
  const col = frame % spec.used;
  const sx = col * CELL_WIDTH;
  const sy = spec.row * CELL_HEIGHT;
  target.clearRect(0, 0, target.canvas.width, target.canvas.height);
  target.imageSmoothingEnabled = true;
  target.drawImage(source, sx, sy, CELL_WIDTH, CELL_HEIGHT, offsetX, offsetY, CELL_WIDTH * scale, CELL_HEIGHT * scale);
}

// Draw the plane sprite for a look-direction cell onto a target canvas.
export function drawLookCell(
  source: HTMLCanvasElement,
  target: CanvasRenderingContext2D,
  direction: LookDirection,
  scale: number,
  offsetX: number,
  offsetY: number,
): void {
  const { row, col } = lookSlot(direction);
  const sx = col * CELL_WIDTH;
  const sy = row * CELL_HEIGHT;
  target.clearRect(0, 0, target.canvas.width, target.canvas.height);
  target.imageSmoothingEnabled = true;
  target.drawImage(source, sx, sy, CELL_WIDTH, CELL_HEIGHT, offsetX, offsetY, CELL_WIDTH * scale, CELL_HEIGHT * scale);
}
