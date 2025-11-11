#![cfg_attr(not(debug_assertions), windows_subsystem = "macos")]

use std::{
    ffi::c_void,
    sync::{Arc, Mutex},
};

use tauri::State;
use enigo::{Enigo, Direction, Button, Key, Coordinate};
use once_cell::sync::Lazy;

// ==========================================
// Overlay Manager
// ==========================================

struct OverlayManager {
    overlays: Arc<Mutex<Vec<*mut c_void>>>,
}

impl OverlayManager {
    fn new() -> Self {
        Self {
            overlays: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn add_overlay(&self, overlay: *mut c_void) {
        let mut overlays = self.overlays.lock().unwrap();
        overlays.push(overlay);
    }

    fn destroy_all(&self) {
        let mut overlays = self.overlays.lock().unwrap();
        overlays.clear();
    }
}

unsafe impl Send for OverlayManager {}
unsafe impl Sync for OverlayManager {}

static OVERLAY_MANAGER: Lazy<OverlayManager> = Lazy::new(OverlayManager::new);

// ==========================================
// MOUSE & KEYBOARD COMMANDS
// ==========================================

#[tauri::command]
fn mouse_move(x: i32, y: i32) -> Result<(), String> {
    let mut enigo = Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn mouse_click(button: String) -> Result<(), String> {
    let mut enigo = Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;

    let btn = match button.to_lowercase().as_str() {
        "left" => Button::Left,
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => return Err("Unknown mouse button".to_string()),
    };

    enigo.button(btn, Direction::Click).map_err(|e| e.to_string())
}

#[tauri::command]
fn key_press(text: String) -> Result<(), String> {
    let mut enigo = Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;

    for ch in text.chars() {
        // Send each character
        enigo.key(Key::Unicode(ch), Direction::Click).map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ==========================================
// PRIVACY OVERLAY (macOS-specific)
// ==========================================

#[cfg(target_os = "macos")]
mod macos_overlay {
    use super::*;
    use cocoa::appkit::{
        NSApp, NSApplication, NSBackingStoreType, NSColor, NSView, NSWindow, NSWindowStyleMask,
    };
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSAutoreleasePool, NSRect};
    use objc::{class, msg_send, sel, sel_impl};

    pub fn create_privacy_overlay() -> Result<*mut c_void, String> {
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);

            let app: id = NSApp();
            app.activateIgnoringOtherApps_(true);

            let screen: id = msg_send![class!(NSScreen), mainScreen];
            let frame: NSRect = msg_send![screen, frame];

            let window: id = msg_send![class!(NSWindow), alloc];
            let style_mask = NSWindowStyleMask::NSBorderlessWindowMask;

            let overlay: id = window
                .initWithContentRect_styleMask_backing_defer_(
                    frame,
                    style_mask,
                    NSBackingStoreType::NSBackingStoreBuffered,
                    false,
                );

            overlay.setBackgroundColor_(NSColor::colorWithCalibratedRed_green_blue_alpha_(
                nil, 0.0, 0.0, 0.0, 0.6,
            ));
            overlay.setLevel_((i32::MAX as i64)); // fixed type
            overlay.makeKeyAndOrderFront_(nil);

            Ok(overlay as *mut c_void)
        }
    }

    pub fn destroy_privacy_overlay(manager: &super::OverlayManager) {
        unsafe {
            let overlays = manager.overlays.lock().unwrap();
            for overlay_ptr in overlays.iter() {
                let window: id = *overlay_ptr as id;
                let _: () = msg_send![window, close];
            }
        }
    }
}

#[tauri::command]
fn create_privacy_overlay(state: State<'_, OverlayManager>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        match macos_overlay::create_privacy_overlay() {
            Ok(ptr) => {
                state.add_overlay(ptr);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Privacy overlay is only supported on macOS".to_string())
    }
}

#[tauri::command]
fn destroy_privacy_overlay(state: State<'_, OverlayManager>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos_overlay::destroy_privacy_overlay(&state);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Privacy overlay is only supported on macOS".to_string())
    }
}

// ==========================================
// TAURI ENTRY
// ==========================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(OverlayManager::new())
        .invoke_handler(tauri::generate_handler![
            mouse_move,
            mouse_click,
            key_press,
            create_privacy_overlay,
            destroy_privacy_overlay
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
