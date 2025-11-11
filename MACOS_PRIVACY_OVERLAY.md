# macOS Privacy Overlay Implementation

This document explains how the privacy overlay feature works on macOS in the ScreenShare application.

## Overview

The privacy overlay feature creates a full-screen overlay window that excludes itself from screen capture. This ensures that sensitive information on the host screen is not visible to viewers during a screen sharing session.

## Implementation Details

### macOS-Specific Code

The macOS implementation uses the following Cocoa APIs:

1. **NSWindow**: Creates a borderless window that covers the entire screen
2. **NSWindow.setSharingType()**: Sets the window sharing type to `NSWindowSharingNone` (0) to exclude it from screen capture
3. **NSWindow.setIgnoresMouseEvents()**: Makes the window click-through so it doesn't interfere with user interaction
4. **NSWindow.setLevel()**: Sets the window level to ensure it stays on top of other windows

### Key Features

- **Screen Capture Exclusion**: The overlay window is excluded from screen capture using the `setSharingType` method
- **Click-Through**: The window ignores mouse events, allowing users to interact with applications underneath
- **Full-Screen Coverage**: The overlay covers the entire screen to ensure complete privacy
- **Visual Indication**: The overlay is rendered as a black screen with a loading indicator

## Platform Differences

### Windows vs macOS

| Feature | Windows | macOS |
|---------|---------|-------|
| API Used | Win32 API | Cocoa API |
| Screen Capture Exclusion | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` | `setSharingType(0)` |
| Window Creation | `CreateWindowEx` | `NSWindow.init` |
| Click-Through | `WS_EX_TRANSPARENT` style | `setIgnoresMouseEvents(true)` |

## Building for macOS

To build the application for macOS:

1. Ensure you have the required dependencies in `Cargo.toml`:
   ```toml
   [target."cfg(target_os = \"macos\")".dependencies]
   cocoa = "0.25"
   objc = "0.2"
   ```

2. Update the build script in `build.rs`:
   ```rust
   #[cfg(target_os = "macos")]
   {
       println!("cargo:rustc-link-lib=framework=Cocoa");
       println!("cargo:rustc-link-lib=framework=CoreGraphics");
       println!("cargo:rustc-link-lib=framework=AppKit");
   }
   ```

3. Configure Tauri for macOS in `tauri.conf.json`:
   ```json
   "bundle": {
     "macOS": {
       "minimumSystemVersion": "10.15"
     }
   }
   ```

## Limitations

1. **macOS Version**: The privacy overlay feature requires macOS 10.15 (Catalina) or later
2. **Screen Recording Permission**: The application may need screen recording permissions to function properly
3. **Development Testing**: This implementation can only be fully tested on a macOS machine

## Future Improvements

1. Add animated loading indicator similar to the Windows implementation
2. Implement proper window cleanup and disposal
3. Add support for multiple displays
4. Improve error handling and logging