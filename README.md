# RustJay Template

A high-performance video processing template for RustJay VJ applications, built with Rust and wgpu.

## Features

- **Single Video Input** with hot-swappable sources:
  - Webcam (via nokhwa)
  - NDI (Network Device Interface)
  - Syphon (macOS GPU texture sharing)
  
- **Native BGRA Format** throughout for optimal macOS performance

- **HSB Color Manipulation** in real-time:
  - Hue Shift (-180° to +180°)
  - Saturation Multiplier (0x to 2x)
  - Brightness Multiplier (0x to 2x)

- **Audio Analysis**:
  - 8-band FFT
  - Beat detection
  - Volume monitoring

- **Multiple Outputs**:
  - NDI network output
  - Syphon output (macOS)

- **Dual-Window Architecture**:
  - Control window with ImGui interface
  - Fullscreen-capable output window with hidden cursor

## Quick Start

```bash
# Build the application
cd rustjay-template
cargo build --release

# Run with default features (webcam support)
cargo run --release

# Run without webcam support (if libclang is not available)
cargo run --release --no-default-features
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Shift+F` | Toggle fullscreen on output window |
| `Escape` | Exit application |

## Architecture

```
rustjay-template/
├── src/
│   ├── main.rs           # Entry point
│   ├── app.rs            # Main application handler (winit)
│   ├── core/
│   │   ├── mod.rs
│   │   ├── state.rs      # Shared state between threads
│   │   └── vertex.rs     # GPU vertex types
│   ├── input/
│   │   ├── mod.rs        # Input manager
│   │   ├── ndi.rs        # NDI input receiver
│   │   ├── webcam.rs     # Webcam capture
│   │   └── syphon_input.rs # Syphon input (macOS)
│   ├── audio/
│   │   └── mod.rs        # Audio analysis
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── renderer.rs   # wgpu render engine
│   │   ├── texture.rs    # Texture utilities
│   │   └── shaders/
│   │       └── main.wgsl # HSB color shader
│   ├── gui/
│   │   ├── mod.rs
│   │   ├── gui.rs        # ImGui interface
│   │   └── renderer.rs   # ImGui wgpu renderer
│   └── output/
│       ├── mod.rs        # Output manager
│       ├── ndi_output.rs # NDI output sender
│       └── syphon_output.rs # Syphon output (macOS)
├── Cargo.toml
└── README.md
```

## Dependencies

### Required
- Rust 1.70+
- macOS (for Syphon support), Windows, or Linux

### Optional
- libclang (for nokhwa webcam support)
- NDI Runtime (for NDI input/output)

## Performance Considerations

1. **BGRA Format**: All textures use BGRA8 format which is native on macOS, avoiding color space conversions.

2. **Zero-Copy Paths**: 
   - Syphon input uses GPU-to-GPU texture copying
   - Syphon output publishes textures directly without readback

3. **Dedicated Threads**:
   - Input sources run on separate threads
   - Audio analysis runs on separate thread
   - Rendering happens on main thread

## Customization

### Adding New Input Sources

1. Create a new module in `src/input/`
2. Implement the input trait pattern
3. Add to `InputManager` in `src/input/mod.rs`

### Adding Shader Effects

1. Modify `src/engine/shaders/main.wgsl`
2. Add uniforms to `HsbUniforms` struct
3. Update GUI controls in `src/gui/gui.rs`

## License

MIT License - See LICENSE file for details

## Acknowledgments

- Built with [wgpu](https://wgpu.rs/) for cross-platform GPU acceleration
- Uses [Dear ImGui](https://github.com/ocornut/imgui) for the UI
- NDI support via [grafton-ndi](https://crates.io/crates/grafton-ndi)
- Syphon support via [syphon-core](https://github.com/syphon-org/syphon-rs)
