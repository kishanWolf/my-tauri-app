fn main() {
    // Enable macOS-specific features
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=AppKit");
    }
    
    tauri_build::build()
}