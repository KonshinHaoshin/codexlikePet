use tauri::Manager;

const LOOK_MARGIN_LOGICAL: f64 = 72.0;
const LOOK_DEADZONE_LOGICAL: f64 = 60.0;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

/// Returns the 16-way look-direction index (0..=15) for the cursor when it is
/// inside the pet window or its small surrounding look area.
///
/// - `0`        => looking up       (12 o'clock)
/// - then clockwise in 22.5deg steps, matching the Codex v2 look contract.
/// - `None`     => cursor is outside the local look area or inside the pet
///   deadzone; the app should fall back to idle.
#[tauri::command]
fn look_direction(app: tauri::AppHandle) -> Option<u8> {
    let cursor = app.cursor_position().ok()?;
    let window = app.get_webview_window("main")?;
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let scale_factor = window.scale_factor().ok()?;

    let left = pos.x as f64;
    let top = pos.y as f64;
    let right = left + size.width as f64;
    let bottom = top + size.height as f64;

    // Only react while the pointer is over the pet window or close to one of
    // its edges. All coordinates here are physical because Tauri's global
    // cursor and outer-window APIs use physical pixels.
    let dx_to_window = if cursor.x < left {
        left - cursor.x
    } else if cursor.x > right {
        cursor.x - right
    } else {
        0.0
    };
    let dy_to_window = if cursor.y < top {
        top - cursor.y
    } else if cursor.y > bottom {
        cursor.y - bottom
    } else {
        0.0
    };
    let look_margin = LOOK_MARGIN_LOGICAL * scale_factor;
    if dx_to_window * dx_to_window + dy_to_window * dy_to_window > look_margin * look_margin {
        return None;
    }

    let cx = left + size.width as f64 / 2.0;
    let cy = top + size.height as f64 / 2.0;
    let dx = cursor.x - cx;
    let dy = cursor.y - cy;

    // Deadzone: when the cursor is close to the pet center, use the normal
    // idle animation instead of forcing a direction.
    let deadzone = LOOK_DEADZONE_LOGICAL * scale_factor;
    if dx * dx + dy * dy < deadzone * deadzone {
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
