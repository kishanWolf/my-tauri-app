#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::Mutex;
use tauri::Manager;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;

// Removed unused imports

// ----------------------
// Safe HWND wrapper
// ----------------------
#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct SafeHWND(HWND);

// SAFETY: HWND can be shared safely as long as Windows API is called on correct thread
#[cfg(target_os = "windows")]
unsafe impl Send for SafeHWND {}
#[cfg(target_os = "windows")]
unsafe impl Sync for SafeHWND {}

// ----------------------
// Overlay Manager
// ----------------------
struct OverlayManager {
    #[cfg(target_os = "windows")]
    overlays: Mutex<Vec<SafeHWND>>,
    #[cfg(target_os = "macos")]
    ns_windows: Mutex<Vec<*mut std::ffi::c_void>>,
}

impl OverlayManager {
    fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            overlays: Mutex::new(Vec::new()),
            #[cfg(target_os = "macos")]
            ns_windows: Mutex::new(Vec::new()),
        }
    }

    #[cfg(target_os = "windows")]
    fn add_overlay(&self, hwnd: HWND) {
        self.overlays.lock().unwrap().push(SafeHWND(hwnd));
    }
    
    #[cfg(target_os = "macos")]
    fn add_ns_window(&self, ns_window: *mut std::ffi::c_void) {
        self.ns_windows.lock().unwrap().push(ns_window);
    }

    #[cfg(target_os = "windows")]
    fn destroy_all(&self) {
        use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
        for SafeHWND(hwnd) in self.overlays.lock().unwrap().drain(..) {
            unsafe { DestroyWindow(hwnd); }
        }
    }
    
    #[cfg(target_os = "macos")]
    fn destroy_all(&self) {
        // For now, we'll just clear the vector
        // A full implementation would properly dispose of the NSWindow objects
        self.ns_windows.lock().unwrap().clear();
    }
}

// ----------------------
// Privacy overlay functions
// ----------------------
#[cfg(target_os = "windows")]
mod win_privacy {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, COLORREF, RECT};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[allow(non_snake_case)]
    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                
                // Get window dimensions
                let mut rect = RECT::default();
                GetClientRect(hwnd, &mut rect);
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                
                // Create a memory device context for double buffering
                let mem_dc = CreateCompatibleDC(hdc);
                let bitmap = CreateCompatibleBitmap(hdc, width, height);
                let old_bitmap = SelectObject(mem_dc, bitmap);
                
                // Fill background with black
                let brush = CreateSolidBrush(COLORREF(0));
                FillRect(mem_dc, &rect, brush);
                DeleteObject(brush);
                
                // Draw loading text
                let text = "Loading...";
                let mut text_rect = rect;
                text_rect.top = height / 2 + 30;
                text_rect.bottom = text_rect.top + 30;
                
                // Convert text to UTF-16 for Windows API
                let mut wide_text: Vec<u16> = text.encode_utf16().collect();
                wide_text.push(0); // Null terminator
                
                SetTextColor(mem_dc, COLORREF(0x00FFFFFF)); // White text
                SetBkMode(mem_dc, TRANSPARENT);
                DrawTextW(mem_dc, &mut wide_text, &mut text_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                
                // Draw animated spinner
                let center_x = width / 2;
                let center_y = height / 2;
                let radius = 20;
                
                // Get current time for animation
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                let angle = (now / 10) % 360; // Rotate 360 degrees every 3.6 seconds
                
                // Draw the spinner circle
                let pen = CreatePen(PS_SOLID, 3, COLORREF(0x00FFFFFF));
                let old_pen = SelectObject(mem_dc, pen);
                let old_brush = SelectObject(mem_dc, GetStockObject(NULL_BRUSH));
                
                Ellipse(mem_dc, 
                    center_x - radius, 
                    center_y - radius, 
                    center_x + radius, 
                    center_y + radius);
                
                // Draw rotating line to indicate progress
                let angle_rad = (angle as f64) * std::f64::consts::PI / 180.0;
                let end_x = center_x + (radius as f64 * angle_rad.sin()) as i32;
                let end_y = center_y - (radius as f64 * angle_rad.cos()) as i32;
                
                MoveToEx(mem_dc, center_x, center_y, None);
                LineTo(mem_dc, end_x, end_y);
                
                SelectObject(mem_dc, old_pen);
                SelectObject(mem_dc, old_brush);
                DeleteObject(pen);
                
                // BitBlt the memory DC to the screen DC
                BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);
                
                // Cleanup
                SelectObject(mem_dc, old_bitmap);
                DeleteObject(bitmap);
                DeleteDC(mem_dc);
                
                EndPaint(hwnd, &ps);
                
                // Schedule next animation frame (every 50ms for smooth animation)
                SetTimer(hwnd, 1, 50, None);
                LRESULT(0)
            },
            WM_TIMER => {
                // Redraw the window
                InvalidateRect(hwnd, None, false);
                LRESULT(0)
            },
            WM_ERASEBKGND => {
                LRESULT(1) // We handle background erasing in WM_PAINT
            },
            WM_DESTROY => {
                KillTimer(hwnd, 1);
                LRESULT(0)
            },
            _ => DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    pub unsafe fn create_overlay(x: i32, y: i32, w: i32, h: i32) -> HWND {
        let h_instance = GetModuleHandleW(None).unwrap();
        let class_name = windows::core::w!("PrivacyNativeOverlay");

        let wc = WNDCLASSW {
            hInstance: h_instance.into(),
            lpszClassName: PCWSTR(class_name.0),
            lpfnWndProc: Some(wnd_proc),
            hbrBackground: CreateSolidBrush(COLORREF(0)),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PCWSTR(class_name.0),
            PCWSTR::null(),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            h_instance,
            None,
        )
        .expect("Failed to create overlay");

        hwnd
    }

    pub unsafe fn apply_privacy(hwnd: HWND) {
        const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;
        let _ = SetWindowDisplayAffinity(hwnd, windows::Win32::UI::WindowsAndMessaging::WINDOW_DISPLAY_AFFINITY(WDA_EXCLUDEFROMCAPTURE));
    }

    pub unsafe fn make_click_through(hwnd: HWND) {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize,
        );
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

#[cfg(target_os = "macos")]
mod mac_privacy {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSAutoreleasePool, NSRect, NSSize, NSPoint};
    use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSBackingStoreType, NSView, 
                        NSColor};
    use objc::runtime::{Object};
    use objc::{msg_send, sel, sel_impl, class};
    use std::ffi::c_void;
    
    // Custom view for drawing the privacy overlay
    pub struct PrivacyOverlayView {
        pub objc: id,
    }
    
    impl PrivacyOverlayView {
        pub fn new(frame: NSRect) -> Self {
            let pool = unsafe { NSAutoreleasePool::new(nil) };
            
            // Create custom view
            let view: id = unsafe {
                msg_send![class!(NSView), alloc]
            };
            
            let view: id = unsafe {
                msg_send![view, initWithFrame: frame]
            };
            
            // Set background color to black
            let black_color: id = unsafe {
                msg_send![class!(NSColor), blackColor]
            };
            
            unsafe {
                let () = msg_send![view, setBackgroundColor: black_color];
            }
            
            // Enable drawing
            unsafe {
                let () = msg_send![view, setWantsLayer: true];
            }
            
            std::mem::drop(pool);
            
            PrivacyOverlayView { objc: view }
        }
    }
    
    // Create an overlay window that excludes itself from screen capture
    pub fn create_overlay(x: i32, y: i32, w: i32, h: i32) -> *mut c_void {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            
            // Create window frame
            let frame = NSRect::new(
                NSPoint::new(x as f64, y as f64),
                NSSize::new(w as f64, h as f64)
            );
            
            // Create window with specific style for privacy overlay
            let window_style = NSWindowStyleMask::NSBorderlessWindowMask;
            let window: id = msg_send![class!(NSWindow), alloc];
            let window: id = msg_send![
                window,
                initWithContentRect:frame
                styleMask:window_style
                backing:NSBackingStoreType::NSBackingStoreBuffered
                defer:false
            ];
            
            // Set window level to floating so it stays on top
            let () = msg_send![window, setLevel: 3]; // NSFloatingWindowLevel
            
            // Make window opaque and set background color
            let () = msg_send![window, setOpaque: true];
            
            let black_color: id = msg_send![class!(NSColor), blackColor];
            let () = msg_send![window, setBackgroundColor: black_color];
            
            // Create and set content view
            let view = PrivacyOverlayView::new(frame);
            let () = msg_send![window, setContentView: view.objc];
            
            // Make window visible
            let () = msg_send![window, makeKeyAndOrderFront: nil];
            
            std::mem::drop(pool);
            
            // Return pointer to window
            window as *mut c_void
        }
    }
    
    // Apply privacy settings to exclude window from screen capture
    pub fn apply_privacy(ns_window_ptr: *mut c_void) {
        if ns_window_ptr.is_null() {
            return;
        }
        
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let ns_window: id = std::mem::transmute(ns_window_ptr);
            
            // On macOS, we can use the NSWindow property to exclude from screen capture
            // This is available from macOS 10.15+ (Catalina)
            let () = msg_send![ns_window, setSharingType: 1]; // NSWindowSharingNone
            
            std::mem::drop(pool);
        }
    }
    
    // Make the window click-through (transparent to mouse events)
    pub fn make_click_through(ns_window_ptr: *mut c_void) {
        if ns_window_ptr.is_null() {
            return;
        }
        
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let ns_window: id = std::mem::transmute(ns_window_ptr);
            
            // Set window to ignore mouse events
            let () = msg_send![ns_window, setIgnoresMouseEvents: true];
            
            // Set window alpha to make it fully opaque but still visible
            let () = msg_send![ns_window, setAlphaValue: 1.0];
            
            std::mem::drop(pool);
        }
    }
}

// ----------------------
// Tauri commands
// ----------------------
#[tauri::command]
fn create_privacy_overlay(manager: tauri::State<OverlayManager>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use win_privacy::*;

        // For simplicity, make a single overlay full screen
        let hwnd = create_overlay(0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        apply_privacy(hwnd);
        make_click_through(hwnd);
        manager.add_overlay(hwnd);
        
        // Trigger a repaint to show the loading indicator
        InvalidateRect(hwnd, None, true);
    }
    
    #[cfg(target_os = "macos")]
    {
        use mac_privacy::*;
        
        // Get screen dimensions for full-screen overlay
        // This is a simplified approach - in practice, you'd want to get the actual screen size
        let screen_width = 1920;  // Default fallback
        let screen_height = 1080; // Default fallback
        
        // Create full-screen overlay
        let ns_window = create_overlay(0, 0, screen_width, screen_height);
        apply_privacy(ns_window);
        make_click_through(ns_window);
        manager.add_ns_window(ns_window);
    }
    
    Ok(())
}

#[tauri::command]
fn destroy_privacy_overlay(manager: tauri::State<OverlayManager>) -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    manager.destroy_all();
    Ok(())
}

// ----------------------
// Keyboard & Mouse commands
// ----------------------
#[tauri::command]
fn mouse_move(x: i32, y: i32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
            MOUSEEVENTF_MOVE, MOUSEEVENTF_ABSOLUTE,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN
        };

        let nx = ((x as f64 / (GetSystemMetrics(SM_CXSCREEN) as f64)) * 65535.0).round() as i32;
        let ny = ((y as f64 / (GetSystemMetrics(SM_CYSCREEN) as f64)) * 65535.0).round() as i32;

        let mut inputs = [INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: nx, dy: ny, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, time: 0, dwExtraInfo: 0 } }
        }];
        let sent = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 { return Err("SendInput failed".into()); }
    }
    #[cfg(not(target_os = "windows"))]
    {
        use enigo::MouseControllable;
        let mut enigo = enigo::Enigo::new();
        enigo.mouse_move_to(x, y);
    }
    Ok(())
}

#[tauri::command]
fn mouse_click(button: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        };
        let (down, up) = match button.as_str() {
            "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        };
        let mut inputs = [
            INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: down, time: 0, dwExtraInfo: 0 } } },
            INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: up, time: 0, dwExtraInfo: 0 } } },
        ];
        let sent = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 { return Err("SendInput failed".into()); }
    }
    #[cfg(not(target_os = "windows"))]
    {
        use enigo::{MouseButton, MouseControllable};
        let mut enigo = enigo::Enigo::new();
        let btn = if button == "right" { MouseButton::Right } else { MouseButton::Left };
        enigo.mouse_click(btn);
    }
    Ok(())
}

#[tauri::command]
fn key_press(key: String) -> Result<(), String> {
    use enigo::KeyboardControllable;
    let mut enigo = enigo::Enigo::new();
    if key.len() == 1 {
        enigo.key_sequence(&key);
    } else {
        enigo.key_sequence(&key);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct Modifiers { alt: bool, ctrl: bool, shift: bool, meta: bool }

#[tauri::command]
fn key_event(action: String, key: String, code: String, mods: Modifiers) -> Result<(), String> {
  #[cfg(target_os = "windows")]
  unsafe {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
      SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
      VK_SHIFT, VK_CONTROL, VK_MENU, VIRTUAL_KEY,
      VK_RETURN, VK_BACK, VK_TAB, VK_ESCAPE, VK_SPACE,
      VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN,
      VK_HOME, VK_END, VK_PRIOR, VK_NEXT, VK_INSERT, VK_DELETE,
      VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4,
      VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9,
      VK_ADD, VK_SUBTRACT, VK_MULTIPLY, VK_DIVIDE, VK_DECIMAL,
      KEYEVENTF_KEYUP, KEYEVENTF_EXTENDEDKEY
    };
    fn vk_from_keycode(key: &str, code: &str) -> (VIRTUAL_KEY, bool) {
      // Return (virtual_key, is_extended)
      match code {
        // Navigation / editing
        "ArrowLeft" => (VIRTUAL_KEY(VK_LEFT.0), true),
        "ArrowRight" => (VIRTUAL_KEY(VK_RIGHT.0), true),
        "ArrowUp" => (VIRTUAL_KEY(VK_UP.0), true),
        "ArrowDown" => (VIRTUAL_KEY(VK_DOWN.0), true),
        "Home" => (VIRTUAL_KEY(VK_HOME.0), true),
        "End" => (VIRTUAL_KEY(VK_END.0), true),
        "PageUp" => (VIRTUAL_KEY(VK_PRIOR.0), true),
        "PageDown" => (VIRTUAL_KEY(VK_NEXT.0), true),
        "Insert" => (VIRTUAL_KEY(VK_INSERT.0), true),
        "Delete" => (VIRTUAL_KEY(VK_DELETE.0), true),
        "Backspace" => (VIRTUAL_KEY(VK_BACK.0), false),
        "Enter" => (VIRTUAL_KEY(VK_RETURN.0), false),
        "NumpadEnter" => (VIRTUAL_KEY(VK_RETURN.0), false),
        "Tab" => (VIRTUAL_KEY(VK_TAB.0), false),
        "Escape" => (VIRTUAL_KEY(VK_ESCAPE.0), false),
        "Space" => (VIRTUAL_KEY(VK_SPACE.0), false),
        // Numpad digits/operators
        "Numpad0" => (VIRTUAL_KEY(VK_NUMPAD0.0), false),
        "Numpad1" => (VIRTUAL_KEY(VK_NUMPAD1.0), false),
        "Numpad2" => (VIRTUAL_KEY(VK_NUMPAD2.0), false),
        "Numpad3" => (VIRTUAL_KEY(VK_NUMPAD3.0), false),
        "Numpad4" => (VIRTUAL_KEY(VK_NUMPAD4.0), false),
        "Numpad5" => (VIRTUAL_KEY(VK_NUMPAD5.0), false),
        "Numpad6" => (VIRTUAL_KEY(VK_NUMPAD6.0), false),
        "Numpad7" => (VIRTUAL_KEY(VK_NUMPAD7.0), false),
        "Numpad8" => (VIRTUAL_KEY(VK_NUMPAD8.0), false),
        "Numpad9" => (VIRTUAL_KEY(VK_NUMPAD9.0), false),
        "NumpadAdd" => (VIRTUAL_KEY(VK_ADD.0), false),
        "NumpadSubtract" => (VIRTUAL_KEY(VK_SUBTRACT.0), false),
        "NumpadMultiply" => (VIRTUAL_KEY(VK_MULTIPLY.0), false),
        "NumpadDivide" => (VIRTUAL_KEY(VK_DIVIDE.0), true),
        "NumpadDecimal" => (VIRTUAL_KEY(VK_DECIMAL.0), false),
        _ => {
          match key {
            "Control" => (VIRTUAL_KEY(VK_CONTROL.0), true),
            "Shift" => (VIRTUAL_KEY(VK_SHIFT.0), false),
            "Alt" => (VIRTUAL_KEY(VK_MENU.0), true),
            _ => {
              // Basic mapping for letters and digits
              let upper = key.to_uppercase();
              if upper.len() == 1 {
                let ch = upper.chars().next().unwrap() as u16;
                return (VIRTUAL_KEY(ch), false);
              }
              (VIRTUAL_KEY(0), false)
            }
          }
        }
      }
    }
    let mut inputs: Vec<INPUT> = Vec::new();
    // apply modifiers if needed (down before, up after)
    let mut push_key = |vk: VIRTUAL_KEY, up: bool, extended: bool| {
      let mut flags = if up { KEYEVENTF_KEYUP } else { Default::default() };
      if extended { flags |= KEYEVENTF_EXTENDEDKEY; }
      inputs.push(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } });
    };
    if action == "down" {
      if mods.ctrl { push_key(VIRTUAL_KEY(VK_CONTROL.0), false, true); }
      if mods.shift { push_key(VIRTUAL_KEY(VK_SHIFT.0), false, false); }
      if mods.alt { push_key(VIRTUAL_KEY(VK_MENU.0), false, true); }
    }
    let (vk, ext) = vk_from_keycode(&key, &code);
    if vk.0 != 0 { push_key(vk, action == "up", ext); }
    if action == "up" {
      if mods.alt { push_key(VIRTUAL_KEY(VK_MENU.0), true, true); }
      if mods.shift { push_key(VIRTUAL_KEY(VK_SHIFT.0), true, false); }
      if mods.ctrl { push_key(VIRTUAL_KEY(VK_CONTROL.0), true, true); }
    }
    if !inputs.is_empty() {
      let mut arr = inputs.into_boxed_slice();
      let sent = SendInput(&mut arr, std::mem::size_of::<INPUT>() as i32);
      if sent == 0 { return Err("SendInput key_event failed".into()); }
    }
  }
  #[cfg(not(target_os = "windows"))]
  {
    use enigo::{KeyboardControllable, Key, KeyDirection};
    let mut enigo = enigo::Enigo::new();
    if mods.ctrl { enigo.key(Key::Control, KeyDirection::Press); }
    if mods.shift { enigo.key(Key::Shift, KeyDirection::Press); }
    if mods.alt { enigo.key(Key::Alt, KeyDirection::Press); }
    if action == "down" { enigo.key_sequence(&key); } else { /* best-effort */ }
    if mods.alt { enigo.key(Key::Alt, KeyDirection::Release); }
    if mods.shift { enigo.key(Key::Shift, KeyDirection::Release); }
    if mods.ctrl { enigo.key(Key::Control, KeyDirection::Release); }
  }
  Ok(())
}

// ----------------------
// Main entry
// ----------------------
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .manage(OverlayManager::new())
        .invoke_handler(tauri::generate_handler![
            mouse_move,
            mouse_click,
            key_press,
            key_event,
            create_privacy_overlay,
            destroy_privacy_overlay
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
