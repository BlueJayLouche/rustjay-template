# Spout Integration Fixes - Implementation Plan

## Status: ✅ IMPLEMENTED (Pending Testing)

## Overview
This document tracks the fixes for Windows Spout integration issues:
1. Input connects but doesn't display texture
2. Output sender not discoverable in other apps

---

## Summary of Changes

### Files Modified

| File | Changes |
|------|---------|
| `src/engine/texture_utils.rs` | ✅ NEW - Row pitch alignment helpers, DXGI constants |
| `src/engine/mod.rs` | ✅ Export texture_utils module |
| `src/engine/texture.rs` | ✅ Import alignment helper (for future use) |
| `src/output/spout_output.rs` | ✅ Early registration, aligned pitch, metadata, constants, logging |
| `src/input/spout_input.rs` | ✅ Fast memcpy path, enhanced logging |

---

## Root Causes & Fixes

### 1. Row Pitch Misalignment (CRITICAL) ✅ FIXED
**Problem:** wgpu requires 256-byte aligned row pitch for COPY operations.

**Solution:**
- Created `texture_utils::aligned_row_pitch_bgra()` helper
- Updated `spout_output.rs::read_texture_bgra()` to use aligned pitch
- Used aligned pitch in `UpdateSubresource()` call

**Code:**
```rust
let bytes_per_row = crate::engine::texture_utils::aligned_row_pitch_bgra(width);
```

### 2. Late Sender Registration (HIGH) ✅ FIXED
**Problem:** Spout sender only registered on first frame.

**Solution:**
- Create placeholder 64x64 texture in `SpoutOutput::new()`
- Register sender immediately
- Resize texture on first real frame

**Code:**
```rust
// In SpoutOutput::new()
output.create_shared_texture(64, 64)?;
log::info!("[Spout] Sender '{}' registered early (placeholder 64x64)", name);
```

### 3. Missing Sender Metadata (MEDIUM) ✅ FIXED
**Problem:** `description` field was zeroed.

**Solution:**
- Populate with executable path using `std::env::current_exe()`

**Code:**
```rust
let mut description = [0u8; 256];
if let Ok(exe_path) = std::env::current_exe() {
    let path_str = exe_path.to_string_lossy();
    let path_bytes = path_str.as_bytes();
    let copy_len = path_bytes.len().min(255);
    description[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
}
```

### 4. Hardcoded DXGI Format (LOW) ✅ FIXED
**Problem:** `format: 87` was a magic number.

**Solution:**
- Created `texture_utils::dxgi_format` module
- Use `dxgi_format::B8G8R8A8_UNORM` constant

**Code:**
```rust
format: dxgi_format::B8G8R8A8_UNORM, // Was: 87
```

### 5. Unaligned Buffer Copy in Input (MEDIUM) ✅ FIXED
**Problem:** Row-by-row copy always used even when not needed.

**Solution:**
- Added fast path for when `row_pitch == dst_row_bytes`
- Use `ptr::copy_nonoverlapping` for full buffer copy

**Code:**
```rust
if row_pitch == dst_row_bytes {
    std::ptr::copy_nonoverlapping(src, self.pixel_buffer.as_mut_ptr(), needed);
} else {
    // Row-by-row fallback
}
```

### 6. Handle Leak on Resize (BUG) ✅ FIXED
**Problem:** Old `_sender_info_map` handle not closed on resize.

**Solution:**
- Close old handle before replacing in `create_shared_texture()`

**Code:**
```rust
if !self._sender_info_map.is_invalid() && !self._sender_info_map.0.is_null() {
    CloseHandle(self._sender_info_map).ok();
}
```

---

## Testing Checklist

### Build Verification
- [x] `cargo check` passes on Windows
- [ ] `cargo build --release` succeeds
- [ ] Cross-platform build works (macOS, Linux)

### Functional Testing
- [ ] Spout sender appears immediately in Spout demo receiver
- [ ] Spout input displays texture from Spout demo sender
- [ ] Resolution changes handled correctly
- [ ] No wgpu validation errors

### Performance Testing
- [ ] 60fps maintained at 1080p
- [ ] No memory leaks (check with task manager)

---

## Architecture Decisions

### ADR-1: Row Pitch Alignment
**Decision:** Align all GPU texture COPY operations to 256 bytes  
**Rationale:** wgpu requirement for `copy_texture_to_buffer`  
**Impact:** Compatible with all backends (D3D12, Vulkan, Metal)

### ADR-2: Early Registration with Placeholder
**Decision:** Register sender immediately with 64x64 texture  
**Rationale:** Eliminates discovery race condition  
**Impact:** Sender visible ~16ms earlier

### ADR-3: DXGI Constants Module
**Decision:** Create `texture_utils::dxgi_format` for constants  
**Rationale:** Eliminate magic numbers, improve maintainability  
**Impact:** Reusable across input/output modules

---

## Debug Logging

New log messages added:

```
[Spout] D3D11 device created for sender 'RustJay'
[Spout] Sender 'RustJay' registered early (placeholder 64x64)
[Spout] Registered 'RustJay' in SpoutSenderNames (slot 0)
[Spout] Sender info written for 'RustJay' 64x64 (handle=0xXXXXXXXX)
[Spout] Shared texture 1920x1080 created, handle=0xXXXXXXXX
[Spout] Resizing shared texture from 64x64 to 1920x1080
[Spout] Frame submitted to 'RustJay' (1920x1080)

[Spout Input] Opening shared texture for 'SenderName'
[Spout Input] Successfully opened 1920x1080 texture from 'SenderName'
[Spout Input] Frame received (1920x1080, pitch=7680)
```

Enable with: `RUST_LOG=debug` or `RUST_LOG=trace`
