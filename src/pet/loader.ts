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

// Fetch + decode a v2 pet package (pet.json + spritesheet) into a source canvas.
export async function loadPet(baseUrl: string): Promise<PetLoaderResult> {
  const manifestRes = await fetch(`${baseUrl}/pet.json`);
  if (!manifestRes.ok) throw new Error(`cannot fetch pet.json: ${manifestRes.status}`);
  const manifest: PetManifest = await manifestRes.json();

  if (manifest.spriteVersionNumber !== 2) {
    throw new Error(`unsupported spriteVersionNumber ${manifest.spriteVersionNumber}; only v2 is supported`);
  }

  const spritesheetRes = await fetch(`${baseUrl}/${manifest.spritesheetPath}`);
  if (!spritesheetRes.ok) throw new Error(`cannot fetch spritesheet: ${spritesheetRes.status}`);

  const blob = await spritesheetRes.blob();
  const bitmap = await createImageBitmap(blob);

  // Normalize the bitmap for drawImage + sub-rect slicing inside a canvas.
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();

  // Sanity check against the v2 contract (1536x2288).
  if (canvas.width !== CELL_WIDTH * COLS || canvas.height !== CELL_HEIGHT * ROWS) {
    console.warn(
      `spritesheet is ${canvas.width}x${canvas.height}; expected ${CELL_WIDTH * COLS}x${CELL_HEIGHT * ROWS}`,
    );
  }
  return { manifest, canvas };
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