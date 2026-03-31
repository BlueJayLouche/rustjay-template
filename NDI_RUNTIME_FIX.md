# NDI Runtime DLL Fix - Cross-Platform Solution

## Problem
On Windows, the application failed to start with error `0xc0000135 (STATUS_DLL_NOT_FOUND)` because the NDI runtime DLL (`Processing.NDI.Lib.x64.dll`) could not be found.

## Root Cause
The NDI SDK on Windows installs runtime DLLs to a versioned subdirectory:
- `C:\Program Files\NDI\NDI 6 Runtime\v6\Processing.NDI.Lib.x64.dll`

These paths are not automatically added to the system PATH, causing the DLL load failure.

## Solution

### Cross-Platform Module: `src/ndi_runtime.rs`

Created a new module that:
1. **Windows**: Searches common NDI installation paths and adds the runtime directory to the DLL search path using `SetDllDirectoryW`
2. **macOS/Linux**: No-op (NDI runtime is in standard library paths)

### Changes Made

1. **New file**: `src/ndi_runtime.rs`
   - `init()`: Adds NDI runtime path to DLL search path (Windows only)
   - `is_available()`: Checks if NDI runtime is installed
   - Searches multiple common NDI installation locations

2. **Modified**: `src/main.rs`
   - Added `ndi_runtime` module
   - Calls `ndi_runtime::init()` early in startup (before NDI is used)
   - Logs warning if NDI runtime not found (non-fatal)

3. **Modified**: `Cargo.toml`
   - Added `"Win32_System_LibraryLoader"` feature to Windows dependencies
   - Required for `SetDllDirectoryW` API

## Platform Behavior

| Platform | Behavior |
|----------|----------|
| Windows | Searches for NDI runtime, adds to DLL path, logs warning if not found |
| macOS | No-op (NDI runtime is in standard paths) |
| Linux | No-op (NDI runtime is in standard paths) |

## NDI Search Paths (Windows)

The following paths are searched in order:
1. `C:\Program Files\NDI\NDI 6 Runtime\v6`
2. `C:\Program Files (x86)\NDI\NDI 6 Runtime\v6`
3. `C:\Program Files\NDI\NDI 5 Runtime\v5`
4. `C:\Program Files (x86)\NDI\NDI 5 Runtime\v5`
5. `C:\Program Files\NDI\NDI 4 Runtime\v4`
6. `C:\Program Files (x86)\NDI\NDI 4 Runtime\v4`
7. `C:\Program Files\NDI\NDI 6 SDK\Bin\x64`
8. `C:\Program Files\NDI\NDI 5 SDK\Bin\x64`

## Testing

### Verify Fix
```bash
# Run with NDI feature
cargo run --features ndi

# Check logs for successful initialization
# [INFO] Starting RustJay Template v0.1.0
# [INFO] [NDI Runtime] Added to DLL search path: C:\Program Files\NDI\NDI 6 Runtime\v6
```

### If NDI Not Installed
```bash
cargo run --features ndi
# [WARN] [NDI] Runtime initialization failed: NDI runtime DLL not found...
# [WARN] [NDI] NDI features may not work. Install NDI Tools from https://ndi.tv/tools/
# Application continues without NDI support
```

### Without NDI Feature
```bash
cargo run --no-default-features --features webcam
# Works normally, no NDI code loaded
```

## Backward Compatibility

- **macOS/Linux**: Completely unaffected (module is no-op)
- **Windows without NDI**: Graceful degradation with warning
- **Windows with NDI**: Automatic path detection and setup
- **Build system**: No changes needed for CI/CD

## Future Improvements

1. **Environment variable**: Allow users to specify custom NDI path via `NDI_RUNTIME_DIR`
2. **Registry search**: Query Windows registry for NDI installation path
3. **Static linking**: Investigate static linking of NDI runtime (if license permits)

## References

- NDI Tools: https://ndi.tv/tools/
- Windows DLL Search Order: https://docs.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-search-order
- `SetDllDirectoryW` API: https://docs.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-setdlldirectoryw
