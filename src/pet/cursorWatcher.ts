/**
 * Polls the Rust `look_direction` command (global cursor relative to the
 * transparent pet window center) and reports direction changes.
 *
 * - `number` 0..=15: the 16-way direction index (0 = up, clockwise).
 * - `null`: cursor sits in the pet deadzone -> fall back to idle/front.
 */
export function watchCursorDirection(
  onDirection: (d: number | null) => void,
  intervalMs = 60,
): () => void {
  let last: number | null | undefined = undefined;
  const timer = window.setInterval(async () => {
    let dir: number | null;
    try {
      // @ts-expect-error Tauri injects window.__TAURI_INTERNALS__ when withGlobalTauri is true
      dir = (await window.__TAURI_INTERNALS__.invoke("look_direction")) as number | null;
    } catch {
      dir = null;
    }
    if (dir !== last) {
      last = dir;
      onDirection(dir);
    }
  }, intervalMs);
  return () => window.clearInterval(timer);
}