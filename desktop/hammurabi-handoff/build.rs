fn main() {
    // Tauri codegen only runs when building the desktop-shell binary.
    // Default `cargo check` (feature off) skips this entirely.
    #[cfg(feature = "tauri-app")]
    tauri_build::build();
}
