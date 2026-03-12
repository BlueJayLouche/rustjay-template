# RustJay Template

A high-performance video processing application built with Rust, wgpu, and egui. Features dual-window architecture (control window + fullscreen output), real-time audio analysis, and support for multiple video input/output sources including NDI and Syphon.

## Features

- **Dual-Window Architecture**: Control window (1200x800) + Fullscreen output with internal resolution scaling
- **GPU-Accelerated Rendering**: Built with wgpu 25 for modern, cross-platform graphics
- **Real-Time Audio Analysis**: 8-band spectrum analyzer with beat detection using RealFFT
- **Multiple Input Sources**:
  - Webcam (via nokhwa)
  - NDI (Network Device Interface via grafton-ndi)
  - Syphon (macOS frame sharing - macOS only)
  - Test pattern
- **Multiple Output Destinations**:
  - NDI output
  - Syphon output (macOS only)
- **HSB Color Adjustments**: Real-time Hue/Saturation/Brightness controls via uniform buffers
- **ImGui Interface**: Immediate mode GUI for controls and preview

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         RustJay Template                         │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Input Layer  │  │ Audio Layer  │  │    Output Layer      │  │
│  │  - Webcam    │  │  - CPAL      │  │  - NDI Output        │  │
│  │  - NDI In    │  │  - RealFFT   │  │  - Syphon Output     │  │
│  │  - Syphon In │  │  - 8-bands   │  │  - Screen Output     │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│                      Wgpu Rendering Engine                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Render Target│  │  HSB Shader  │  │   Output Manager     │  │
│  │ (1920x1080)  │  │  (Uniforms)  │  │  (Surface + NDI +    │  │
│  │ Bgra8Unorm   │  │  WGSL        │  │   Syphon)            │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│                     ImGui Control Interface                      │
│                    (imgui-wgpu + imgui-winit)                   │
└─────────────────────────────────────────────────────────────────┘
```

## Requirements

### macOS
- macOS 11.0+ (Big Sur or later)
- Xcode Command Line Tools
- Rust 1.75+
- NDI Runtime (optional, for NDI support)

### Linux
- Vulkan-capable GPU
- Rust 1.75+
- NDI Runtime (optional, for NDI support)

## Building

### Clone and Build

```bash
# Clone the repository
git clone https://github.com/BlueJayLouche/rustjay-template.git
cd rustjay-template

# Build the project
cargo build --release
```

### NDI Support (Optional)

To enable NDI input/output, install the NDI SDK:

1. Download NDI SDK for your platform from [NDI.tv](https://ndi.tv)
2. On macOS, the build system will automatically find it in `/usr/local/lib` or `/Library/NDI SDK for Apple/lib/macOS`

### Syphon Support (macOS Only)

Syphon support requires the Syphon framework. The project includes local syphon crates that handle the integration automatically.

## Running

```bash
# Run in debug mode
cargo run

# Run optimized release build
cargo run --release
```

## Controls

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Esc` | Exit application |
| `Space` | Toggle output fullscreen |
| `1-4` | Switch input source (1=Test Pattern, 2=Webcam, 3=NDI, 4=Syphon) |
| `T` | Toggle test pattern |
| `A` | Toggle audio visualization |

### GUI Controls

The control window provides:
- **Input Selection**: Dropdown to choose video source
- **HSB Adjustments**: Sliders for Hue shift, Saturation, and Brightness
- **Audio Visualization**: Real-time spectrum and beat detection display
- **Output Settings**: Resolution and target selection
- **Preview Windows**: Live preview of input and processed output

## Configuration

Configuration is managed via `config.toml` in the project root:

```toml
[video]
internal_width = 1920
internal_height = 1080
surface_format = "Bgra8Unorm"  # Native macOS format
vsync = true

[audio]
sample_rate = 48000
buffer_size = 1024
fft_size = 2048

[output]
ndi_enabled = true
syphon_enabled = true
fullscreen = false
```

## Project Structure

```
rustjay-template/
├── Cargo.toml           # Project dependencies
├── build.rs             # Build script for framework/library linking
├── src/
│   ├── main.rs          # Application entry point
│   ├── app.rs           # Main application state machine
│   ├── config.rs        # Configuration management
│   ├── audio/
│   │   └── mod.rs       # Audio capture and analysis
│   ├── engine/
│   │   ├── mod.rs       # Rendering engine
│   │   ├── renderer.rs  # Wgpu renderer implementation
│   │   └── shaders/     # WGSL shaders
│   ├── gui/
│   │   ├── mod.rs       # GUI module
│   │   └── renderer.rs  # ImGui wgpu renderer
│   ├── input/
│   │   ├── mod.rs       # Input management
│   │   ├── webcam.rs    # Webcam input (nokhwa)
│   │   ├── ndi.rs       # NDI input (grafton-ndi)
│   │   ├── syphon_input.rs  # Syphon input (macOS)
│   │   └── test_pattern.rs  # Generated test pattern
│   └── output/
│       ├── mod.rs       # Output management
│       ├── ndi_output.rs    # NDI output
│       └── syphon_output.rs # Syphon output (macOS)
```

## Technical Details

### Color Format

The application uses `Bgra8Unorm` throughout for:
- Native macOS performance
- Zero-copy Syphon integration
- Consistent texture formats

### Resolution Pipeline

1. Input sources provide frames at native resolution
2. Render target maintains fixed internal resolution (1920x1080 default)
3. Output manager blits to surface and sends to NDI/Syphon

### Audio Processing

- Uses `realfft` 3.4 with `RealToComplex` trait for FFT
- 8-band frequency analysis
- Beat detection with energy history
- Thread-safe sharing via crossbeam channels

### API Versions

- **wgpu**: 25.0 (adapter request returns `Result`)
- **imgui-wgpu**: 0.25 (texture management via `renderer.textures`)
- **realfft**: 3.4 (uses `RealToComplex` trait)
- **grafton-ndi**: 0.11 (`NDI`, `Finder`, `Receiver` types)

## Troubleshooting

### "Library not loaded" errors

If you encounter dyld errors, ensure:
1. Syphon framework is at `../crates/syphon/syphon-lib/Syphon.framework`
2. NDI SDK is installed in `/usr/local/lib` or `/Library/NDI SDK for Apple/`

The build script automatically sets rpaths for both.

### No video input

Check that your camera/NDI source is available:
```bash
# List available cameras
cargo run -- --list-cameras

# List available NDI sources
cargo run -- --list-ndi
```

### Performance issues

- Run in release mode: `cargo run --release`
- Reduce internal resolution in config.toml
- Disable vsync for lower latency (may cause tearing)

## License

MIT License - See LICENSE file for details

## Credits

Built with:
- [wgpu](https://github.com/gfx-rs/wgpu) - Rust graphics API
- [imgui-rs](https://github.com/imgui-rs/imgui-rs) - ImGui bindings
- [grafton-ndi](https://crates.io/crates/grafton-ndi) - NDI bindings
- [nokhwa](https://github.com/l1npengtul/nokhwa) - Camera capture
- [realfft](https://github.com/HEnquist/realfft) - FFT library
