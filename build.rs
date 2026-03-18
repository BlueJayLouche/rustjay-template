fn main() {
    // Only on macOS
    #[cfg(target_os = "macos")]
    {
        // ===== Syphon Framework =====
        // Resolve framework path relative to this build script so it works on any machine.
        let syphon_framework_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()  // workspace or parent dir
            .and_then(|p| {
                // Try sibling layout: <root>/crates/syphon/syphon-lib
                let candidate = p.join("crates/syphon/syphon-lib");
                if candidate.join("Syphon.framework").exists() { Some(candidate) } else { None }
            })
            .or_else(|| {
                // Fallback: absolute path set via env var SYPHON_FRAMEWORK_DIR
                std::env::var("SYPHON_FRAMEWORK_DIR").ok().map(std::path::PathBuf::from)
            })
            .expect(
                "Syphon.framework not found. Set SYPHON_FRAMEWORK_DIR to the directory \
                 containing Syphon.framework, or place it at <workspace>/../crates/syphon/syphon-lib/"
            );
        let syphon_framework_dir = syphon_framework_dir.to_string_lossy().into_owned();
        
        // Add framework search path
        println!("cargo:rustc-link-arg=-F{}", syphon_framework_dir);
        
        // Link the framework
        println!("cargo:rustc-link-arg=-framework");
        println!("cargo:rustc-link-arg=Syphon");
        
        // Add rpath so it can be found at runtime
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", syphon_framework_dir);
        
        // ===== NDI Library =====
        // libndi.dylib is typically in /usr/local/lib or /Library/NDI\ SDK\ for\ Apple/lib/macOS/
        let ndi_lib_paths = [
            "/usr/local/lib",
            "/Library/NDI SDK for Apple/lib/macOS",
        ];
        
        for path in &ndi_lib_paths {
            if std::path::Path::new(path).exists() {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path);
            }
        }
        
        // Also add common framework/library search paths
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
        
        // Tell cargo to rerun if this build script changes
        println!("cargo:rerun-if-changed=build.rs");
    }
}
