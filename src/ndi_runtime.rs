//! # NDI Runtime Path Setup (Windows)
//!
//! This module ensures the NDI runtime DLL can be found on Windows.
//! It's a no-op on macOS and Linux.
//!
//! ## Why This Exists
//!
//! The NDI SDK on Windows installs runtime DLLs to a versioned subdirectory
//! (e.g., `C:\Program Files\NDI\NDI 6 Runtime\v6\`). These paths are not
//! automatically added to the system PATH, causing "DLL not found" errors
//! when running the application.
//!
//! ## Solution
//!
//! On Windows, we search common NDI installation paths and add the runtime
//! directory to the DLL search path using `SetDllDirectoryW`.
//!
//! ## Platform Notes
//!
//! - **Windows**: Searches for NDI runtime and sets DLL directory
//! - **macOS**: No-op (NDI runtime is in standard library paths)
//! - **Linux**: No-op (NDI runtime is in standard library paths)

#[cfg(target_os = "windows")]
use std::path::Path;

/// Initialize NDI runtime path.
///
/// On Windows, searches for NDI runtime installation and adds it to the
/// DLL search path. On other platforms, this is a no-op.
///
/// # Returns
/// - `Ok(())` if successful or not needed on this platform
/// - `Err(String)` if NDI runtime cannot be found on Windows
///
/// # Example
/// ```
/// // Call this early in your application startup
/// ndi_runtime::init().expect("NDI runtime not found");
/// ```
pub fn init() -> Result<(), String> {
    init_internal()
}

#[cfg(target_os = "windows")]
fn init_internal() -> Result<(), String> {
    use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

    // Search paths for NDI runtime (most common first)
    let search_paths = [
        // NDI 6 Runtime
        "C:\\Program Files\\NDI\\NDI 6 Runtime\\v6",
        "C:\\Program Files (x86)\\NDI\\NDI 6 Runtime\\v6",
        // NDI 5 Runtime
        "C:\\Program Files\\NDI\\NDI 5 Runtime\\v5",
        "C:\\Program Files (x86)\\NDI\\NDI 5 Runtime\\v5",
        // Older versions
        "C:\\Program Files\\NDI\\NDI 4 Runtime\\v4",
        "C:\\Program Files (x86)\\NDI\\NDI 4 Runtime\\v4",
        // SDK paths (development fallback)
        "C:\\Program Files\\NDI\\NDI 6 SDK\\Bin\\x64",
        "C:\\Program Files\\NDI\\NDI 5 SDK\\Bin\\x64",
    ];

    for path_str in &search_paths {
        let path = Path::new(path_str);
        if path.exists() {
            // Check for the actual DLL
            let dll_path = path.join("Processing.NDI.Lib.x64.dll");
            if dll_path.exists() {
                // Convert path to wide string for Windows API
                let wide_path: Vec<u16> = path_str
                    .encode_utf16()
                    .chain(std::iter::once(0)) // null terminator
                    .collect();

                unsafe {
                    // Add to DLL search path
                    if SetDllDirectoryW(windows::core::PCWSTR(wide_path.as_ptr())).is_ok() {
                        log::info!("[NDI Runtime] Added to DLL search path: {}", path_str);
                        return Ok(());
                    }
                }
            }
        }
    }

    // If we get here, we couldn't find the NDI runtime
    Err(format!(
        "NDI runtime DLL not found. Searched paths:\n{}\n\n\
         Please install NDI Tools from https://ndi.tv/tools/",
        search_paths.join("\n")
    ))
}

#[cfg(not(target_os = "windows"))]
fn init_internal() -> Result<(), String> {
    // On macOS and Linux, NDI runtime is installed in standard library paths
    // No special handling needed
    Ok(())
}

/// Check if NDI runtime is available without modifying paths.
///
/// This is useful for displaying a warning at startup without failing.
pub fn is_available() -> bool {
    is_available_internal()
}

#[cfg(target_os = "windows")]
fn is_available_internal() -> bool {
    let search_paths = [
        "C:\\Program Files\\NDI\\NDI 6 Runtime\\v6\\Processing.NDI.Lib.x64.dll",
        "C:\\Program Files\\NDI\\NDI 5 Runtime\\v5\\Processing.NDI.Lib.x64.dll",
        "C:\\Program Files\\NDI\\NDI 6 SDK\\Bin\\x64\\Processing.NDI.Lib.x64.dll",
    ];

    search_paths
        .iter()
        .any(|path| Path::new(path).exists())
}

#[cfg(not(target_os = "windows"))]
fn is_available_internal() -> bool {
    // On macOS/Linux, we assume it's available if the feature is enabled
    // The dynamic linker will report issues at load time
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_does_not_panic() {
        // This should not panic on any platform
        let result = init();
        // On Windows without NDI installed, this will fail
        // On other platforms, it should succeed
        if cfg!(target_os = "windows") {
            // Result depends on whether NDI is installed
            println!("NDI init result: {:?}", result);
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_is_available() {
        // This should not panic
        let available = is_available();
        println!("NDI available: {}", available);
    }
}
