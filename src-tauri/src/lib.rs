use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

/// Returns the 16-way look-direction index (0..=15) for the current global
/// cursor position relative to the main window center.
///
/// - `0`        => looking up       (12 o'clock)
/// - then clockwise in 22.5deg steps, matching the Codex v2 look contract.
/// - `None`     => cursor is inside the pet deadzone (near/over the window),
///   the front-facing neutral area; the app should fall back to idle.
#[tauri::command]
fn look_direction(app: tauri::AppHandle) -> Option<u8> {
    let cursor = app.cursor_position().ok()?;
    let window = app.get_webview_window("main")?;
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;

    let cx = pos.x as f64 + size.width as f64 / 2.0;
    let cy = pos.y as f64 + size.height as f64 / 2.0;
    let dx = cursor.x - cx;
    let dy = cursor.y - cy;

    // Deadzone: when the cursor is close to / over the pet window, treat it as
    // the neutral front-facing case (no direction).
    if dx * dx + dy * dy < 60.0 * 60.0 {
        return None;
    }

    // Angle measured clockwise from "up" in screen coordinates
    // (x grows right, y grows down).
    let deg = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    let index = ((deg + 11.25) / 22.5).floor() as i32 % 16;
    Some(index as u8)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![look_direction])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}