fn main() {
    // Only on macOS
    #[cfg(target_os = "macos")]
    {
        // ===== Syphon Framework =====
        // The framework is at: /Users/alpha/Developer/rust/crates/syphon/syphon-lib/Syphon.framework
        let syphon_framework_dir = "/Users/alpha/Developer/rust/crates/syphon/syphon-lib";
        
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
