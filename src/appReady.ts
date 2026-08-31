import { invoke } from "@tauri-apps/api/core";

const READY_POLL_INTERVAL_MS = 25;
const READY_TIMEOUT_MS = 10_000;

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => globalThis.setTimeout(resolve, milliseconds));
}

/**
 * Configured Tauri windows can load their WebView before the setup hook has
 * finished. Wait until Rust has loaded the persisted config before invoking
 * any command that reads AppState.
 */
export async function waitForAppReady(): Promise<void> {
  const deadline = Date.now() + READY_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      if (await invoke<boolean>("is_app_ready")) return;
    } catch {
      // The invoke handler may not be available until the native runtime is
      // ready. Keep polling during that short startup window.
    }
    await delay(READY_POLL_INTERVAL_MS);
  }
  throw new Error("SakiPet 启动超时，请重新启动应用");
}
