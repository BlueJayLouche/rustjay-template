# RustJay Template

A high-performance video processing application built with Rust, wgpu, and ImGui. Features dual-window architecture (control window + fullscreen output), real-time audio analysis, comprehensive modulation system (LFOs, audio reactivity, MIDI), and support for multiple video input/output sources including NDI and Syphon.

## Quick Start

The easiest way to create a new RustJay project is with the project generator:

```bash
# Install the project generator
cargo install rustjay-new

# Create a new project
rustjay-new my-awesome-vj-app

# Enter the project and run
cd my-awesome-vj-app
cargo run
```

## Features

### Core Video Processing
- **Dual-Window Architecture**: Control window (1200x800) + Fullscreen output with internal resolution scaling
- **GPU-Accelerated Rendering**: Built with wgpu 25 for modern, cross-platform graphics
- **HSB Color Adjustments**: Real-time Hue/Saturation/Brightness controls via uniform buffers
- **ImGui Interface**: Immediate mode GUI for controls and preview

### Input Sources
- **Webcam** (via nokhwa) — on macOS/Windows
- **V4L2** (Linux — real webcams and any `/dev/video*` capture node)
- **NDI** (Network Device Interface via grafton-ndi)
- **Syphon** (macOS GPU texture sharing - macOS only)
- **Spout** (Windows GPU texture sharing - Windows only)
- **Test pattern**

### Output Destinations
- **Screen output** (fullscreen/windowed)
- **NDI output**
- **Syphon output** (macOS only)
- **Spout output** (Windows only)
- **V4L2 loopback output** (Linux only — stream into a virtual webcam)

### Modulation System

#### LFO (Low Frequency Oscillator)
- **3 Independent LFO Banks** with per-parameter assignment
- **Multiple Waveforms**: Sine, Triangle, Ramp, Saw, Square
- **Tempo Sync**: Lock to BPM with beat divisions (1/16 to 8 beats)
- **Phase Offset**: 0-360° (0° aligns with beat)
- **Target Parameters**: Hue Shift, Saturation, Brightness

#### Audio Reactivity
- **8-Band FFT Analysis**: Sub Bass, Bass, Low Mid, Mid, High Mid, High, Very High, Presence
- **Routing Matrix**: Route any FFT band to any HSB parameter
- **Attack/Release Smoothing**: Per-route configurable smoothing
- **Beat Detection**: Automatic BPM estimation with tap tempo

#### External Control
- **MIDI Input**: CC mapping with learn system, device selection
- **OSC (Open Sound Control)**: UDP server with auto-generated addresses
- **Web Remote**: WebSocket-based mobile interface (port 8080)

### Presets & Persistence
- **Quick Slots**: Shift+F1 through Shift+F8 for instant preset recall
- **Save/Load**: Named presets with import/export
- **Auto-Save**: Settings automatically saved to `~/.config/rustjay/settings.json`

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         RustJay Template                                 │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐  │
│  │ Input Layer  │  │ Audio Layer  │  │    Output Layer              │  │
│  │  - Webcam    │  │  - CPAL      │  │  - NDI Output                │  │
│  │  - NDI In    │  │  - RealFFT   │  │  - Spout Output (Win)        │  │
│  │  - Spout In  │  │  - 8-bands   │  │  - Syphon Output (macOS)     │  │
│  │  - Syphon In │  └──────────────┘  │  - Screen Output             │  │
│  └──────────────┘                    └──────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────────┤
│                        Modulation System                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐  │
│  │ LFO Engine   │  │ Audio Router │  │  External Control            │  │
│  │  - 3 Banks   │  │  - FFT→HSB   │  │  - MIDI, OSC, Web            │  │
│  │  - Tempo Sync│  │  - Smoothing │  │  - Parameter Mapping         │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────────┤
│                      Wgpu Rendering Engine                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐  │
│  │ Render Target│  │  HSB Shader  │  │   Output Manager             │  │
│  │ (1920x1080)  │  │  (Uniforms)  │  │  (Surface + NDI + Syphon)    │  │
│  │ Bgra8Unorm   │  │  WGSL        │  │                              │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────────┤
│                     ImGui Control Interface                              │
│                    (imgui-wgpu + imgui-winit)                           │
└─────────────────────────────────────────────────────────────────────────┘
```

### Module Structure

```
src/
├── app/           # Event loop, command dispatch, frame update
├── audio/
│   ├── fft.rs     # Lock-free AudioOutput/AudioConfig + real-time FFT
│   ├── device.rs  # Device enumeration, stream construction (f32/i16/u16)
│   └── routing.rs # Audio-reactive parameter routing
├── core/          # SharedState, HsbParams, LFO, vertex types
├── engine/
│   ├── pipeline.rs  # HSB render pipeline + bind group layouts
│   ├── uniforms.rs  # HsbUniforms GPU type
│   ├── blit.rs      # BlitPipeline (cached — no per-frame allocation)
│   └── texture.rs   # InputTexture, render target helpers
├── gui/           # ImGui tabs (input, color, audio, output, presets…)
├── input/         # Webcam, NDI, Spout (Windows), Syphon (macOS) input sources
├── midi/          # CC mapping with learn system
├── osc/           # UDP OSC server
├── output/        # NDI, Spout (Windows), and Syphon (macOS) output senders
├── presets/       # Preset save/load/apply
└── web/           # WebSocket remote control
```

### Modulation Architecture

The modulation system uses a **separation of concerns** architecture:

```
User Input (GUI/MIDI/Web)
    ↓
Base Values (audio_routing.base_*)
    ↓
[Render Step Composites]
    ├─ Base Values
    ├─ + LFO Modulation
    ├─ + Audio Reactivity
    └─ + External Control
    ↓
Final Shader Parameters
```

This prevents feedback loops and ensures stable base values while allowing expressive real-time modulation.

## Requirements

### macOS
- macOS 11.0+ (Big Sur or later)
- Xcode Command Line Tools
- Rust 1.75+
- NDI Runtime (optional, for NDI support)

### Windows
- Windows 10/11 (64-bit)
- [Rust](https://rustup.rs/) 1.75+ with the `x86_64-pc-windows-msvc` toolchain
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (C++ workload — required by the MSVC toolchain)
- [LLVM](https://github.com/llvm/llvm-project/releases) (optional) — provides `libclang.dll`, required by `bindgen` if building with the `webcam` or `ndi` features. Install the latest LLVM Windows release and set `LIBCLANG_PATH`:
  ```powershell
  set LIBCLANG_PATH=C:\Program Files\LLVM\bin
  ```
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

NDI is not included in the default build. To enable NDI input/output:

```bash
cargo build --release --features ndi
```

You will also need the NDI SDK installed:
1. Download NDI SDK for your platform from [NDI.tv](https://ndi.tv)
2. On macOS, the build system will automatically find it in `/usr/local/lib` or `/Library/NDI SDK for Apple/lib/macOS`
3. On Windows, LLVM must also be installed (NDI's build script uses `bindgen`)

### Syphon Support (macOS Only)

Syphon is enabled automatically on macOS — no feature flag needed. The build system finds the framework at `../syphon-rs/syphon-lib/Syphon.framework`.

**Requirements:** The `syphon-rs` repo must be present as a sibling directory:
```
developer/rust/
├── syphon-rs/          ← must exist
└── rustjay-template/
```

If your layout differs, set `SYPHON_FRAMEWORK_DIR` before building:
```bash
SYPHON_FRAMEWORK_DIR=/path/to/syphon-rs/syphon-lib cargo build --release
```

### Spout Support (Windows Only)

Spout is enabled automatically on Windows. No extra dependencies are required — the Spout sender protocol is implemented directly using the Windows D3D11 and DXGI APIs (via the `windows` crate). No C++ toolchain extras or LLVM are needed.

**Build and run**

```powershell
cargo build --release
cargo run --release
```

Open Resolume Arena, OBS (with [OBS-Spout2-Plugin](https://github.com/Off-World-Live/obs-spout2-plugin)), or any Spout-capable app on the same machine and verify texture sharing works via the **Input** and **Output** tabs.

### V4L2 Support (Linux Only)

On Linux, video I/O uses Video4Linux2 (V4L2):

- **V4L2 Input** — real webcams and any `/dev/video*` capture node are enumerated natively in the **Input** tab.
- **V4L2 Output** — frames are written to a virtual camera device created by the [`v4l2loopback`](https://github.com/umlaeute/v4l2loopback) kernel module. Other apps (OBS, ffplay, Chromium, Firefox) then read from the virtual camera as if it were a real webcam.

#### 1. Install `v4l2loopback`

```bash
# Arch / Manjaro
sudo pacman -S v4l2loopback-dkms v4l-utils

# Debian / Ubuntu
sudo apt install v4l2loopback-dkms v4l-utils
```

#### 2. Load the kernel module with a virtual device

```bash
sudo modprobe v4l2loopback devices=1 video_nr=10 \
    card_label="RustJay Output" exclusive_caps=1
```

Flags:
- `video_nr=10` — the virtual device node, so it appears as `/dev/video10`. Pick any free number.
- `card_label="RustJay Output"` — the name that consumer apps (OBS, browsers) will display.
- `exclusive_caps=1` — required for Chromium/Firefox to recognize the device as a webcam.

Verify it worked:

```bash
v4l2-ctl --list-devices
# → RustJay Output (platform:v4l2loopback-000):
#       /dev/video10
```

Make it persist across reboots:

```bash
echo "v4l2loopback" | sudo tee /etc/modules-load.d/v4l2loopback.conf
echo "options v4l2loopback devices=1 video_nr=10 card_label=\"RustJay Output\" exclusive_caps=1" \
  | sudo tee /etc/modprobe.d/v4l2loopback.conf
```

#### 3. Start the stream

```bash
cargo run --release
```

In the control window → **Output** tab:
1. The "V4L2 Loopback Output" section lists all detected virtual cameras in a combo box.
2. Select `RustJay Output (/dev/video10)` and click **Start V4L2 Output**.
3. The status indicator turns green when streaming.

Consume the stream from any webcam-aware app:

```bash
ffplay /dev/video10
```

Or in OBS: **Sources → Video Capture Device → Device: RustJay Output**.
Or in a browser: open any webcam test site and pick "RustJay Output" from the camera list.

#### Format notes

- Frames are written as **YUYV 4:2:2** — the most compatible format across browsers, OBS, and ffmpeg-based tools.
- BGRA→YUYV conversion is done on the CPU with a pre-allocated scratch buffer (no per-frame allocation) using BT.601 limited-range coefficients.
- If the render resolution changes while the output is running, the V4L2 device is automatically reopened at the new size.

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
| `1-5` | Switch input source (1=Test Pattern, 2=Webcam, 3=NDI, 4=Syphon, 5=Spout) |
| `T` | Toggle test pattern |
| `A` | Toggle audio visualization |
| `Shift+F1-F8` | Load quick preset slot 1-8 |
| `F1-F8` | Save to quick preset slot 1-8 |

### GUI Tabs

#### Input Tab
- Device selection (webcam, NDI, Spout on Windows, Syphon on macOS, V4L2 on Linux)
- Refresh devices button
- Input status display

#### Color Tab
- **HSB Adjustments**: Hue shift (-180° to 180°), Saturation (0-2x), Brightness (0-2x)
- **LFO Window**: Open to configure 3 LFO banks with tempo sync and waveform selection

#### Audio Tab
- Audio device selection
- Amplitude, smoothing, normalization controls
- 8-band FFT visualization
- **Routing Matrix**: Open to create audio→parameter routes
- Tap tempo button

#### Output Tab
- NDI output name and toggle
- Spout output name and toggle (Windows)
- Syphon output name and toggle (macOS)
- V4L2 loopback device selector + toggle (Linux)
- Fullscreen toggle

#### Presets Tab
- Quick slot buttons (F1-F8)
- Save/load/delete named presets
- Import/export presets

#### MIDI Tab
- Device selection
- Learn mode for CC mapping
- Clear all mappings

#### OSC Tab
- Server start/stop (port 9000)
- Auto-generated address display
- Status indicator

#### Web Tab
- Web server start/stop (port 8080)
- Access URL display with local IP
- Connection status

## Web Remote Interface

When the web server is enabled, access the remote control interface from any device on the same network:

1. Start the web server from the **Web** tab
2. Open the displayed URL on your phone/tablet (e.g., `http://192.168.1.100:8080/rustjay`)
3. Control parameters in real-time with a mobile-optimized touch interface

Features:
- Real-time bidirectional sync
- Multiple clients can connect simultaneously
- Auto-generated controls for all parameters

## OSC Addresses

When OSC is enabled, the following addresses are available:

```
/rustjay/color/hue_shift      f  (-180.0 to 180.0)
/rustjay/color/saturation     f  (0.0 to 2.0)
/rustjay/color/brightness     f  (0.0 to 2.0)
/rustjay/color/enabled        f  (0.0 or 1.0)
/rustjay/audio/amplitude      f  (0.0 to 5.0)
/rustjay/audio/smoothing      f  (0.0 to 1.0)
/rustjay/audio/enabled        f  (0.0 or 1.0)
/rustjay/output/fullscreen    f  (0.0 or 1.0)
```

## Configuration

Settings are automatically saved to `~/.config/rustjay/settings.json` and include:
- Window positions and sizes
- Last used devices and sources
- HSB parameter values
- LFO configurations
- Audio routing matrix
- MIDI mappings
- Preset data

### Manual Configuration

You can also create a `config.toml` in the project root:

```toml
[video]
internal_width = 1920
internal_height = 1080
surface_format = "Bgra8Unorm"
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
├── Cargo.toml              # Project dependencies
├── build.rs                # Build script (auto-detects Syphon/NDI paths)
├── src/
│   ├── main.rs             # Application entry point
│   ├── app/
│   │   ├── mod.rs          # App struct, new(), startup, shutdown
│   │   ├── commands.rs     # Input/audio/MIDI/OSC/web command handlers
│   │   ├── update.rs       # Per-frame update methods
│   │   └── events.rs       # winit ApplicationHandler impl
│   ├── audio/
│   │   ├── mod.rs          # Lock-free audio capture and FFT analysis
│   │   └── routing.rs      # Audio→parameter routing matrix
│   ├── config/
│   │   └── mod.rs          # Atomic settings persistence (write-then-rename)
│   ├── core/
│   │   ├── mod.rs          # Core types and shared state
│   │   ├── lfo.rs          # LFO engine and modulation
│   │   ├── state.rs        # Shared application state
│   │   └── vertex.rs       # Vertex data structures
│   ├── engine/
│   │   ├── mod.rs          # Rendering engine
│   │   ├── renderer.rs     # Wgpu renderer implementation
│   │   └── texture.rs      # Texture management
│   ├── gui/
│   │   ├── mod.rs          # GUI module exports
│   │   ├── renderer.rs     # ImGui wgpu renderer
│   │   └── gui/
│   │       ├── gui.rs      # ControlGui struct, top-level layout
│   │       ├── tab_input.rs    # Input tab + preview panels
│   │       ├── tab_color.rs    # Color/HSB tab
│   │       ├── tab_audio.rs    # Audio tab + routing matrix
│   │       ├── tab_output.rs   # Output tab
│   │       ├── tab_settings.rs # Settings + Presets tabs
│   │       ├── tab_control.rs  # MIDI, OSC, Web tabs
│   │       └── tab_lfo.rs      # LFO window
│   ├── input/
│   │   ├── mod.rs          # Input management
│   │   ├── webcam.rs       # Webcam input (nokhwa)
│   │   ├── ndi.rs          # NDI input with source-loss detection
│   │   └── syphon_input.rs # Syphon input — shares main wgpu device
│   ├── midi/
│   │   └── mod.rs          # MIDI input, learn system, device hot-swap
│   ├── osc/
│   │   └── mod.rs          # OSC server
│   ├── output/
│   │   ├── mod.rs          # Output management
│   │   ├── ndi_output.rs   # NDI output
│   │   └── syphon_output.rs# Syphon output (macOS)
│   ├── presets/
│   │   └── mod.rs          # Preset save/load system
│   └── web/
│       ├── mod.rs          # Web server and WebSocket
│       └── ui.html         # Embedded web interface
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
- 8-band frequency analysis (20Hz - 16kHz)
- Beat detection with energy history
- **Lock-free audio path**: FFT results, volume, and beat state are shared via `AtomicU32`/`AtomicBool` — no mutex on the real-time audio thread

### LFO System

- Phase accumulator-based (no time drift)
- Tempo sync uses beat divisions relative to quarter note
- Waveforms: Sine, Triangle, Ramp, Saw, Square
- Modulation applied as additive offset at render time

### API Versions

- **wgpu**: 25.0
- **imgui-wgpu**: 0.25
- **realfft**: 3.4
- **grafton-ndi**: 0.11
- **axum**: 0.7 (web server)
- **midir**: 0.10 (MIDI)

## Troubleshooting

### `Unable to find libclang` (Windows)

The NDI (`grafton-ndi`) and webcam (`nokhwa`) crates use `bindgen`, which requires `libclang.dll` at build time. If you see:

```
Unable to find libclang: "couldn't find any valid shared libraries matching: ['clang.dll', 'libclang.dll']"
```

**Fix:** Install [LLVM for Windows](https://github.com/llvm/llvm-project/releases) (the `LLVM-*-win64.exe` installer) and ensure `LIBCLANG_PATH` points to the `bin` directory:

```powershell
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
cargo build --release
```

**Alternative:** The default build does not require LLVM. This error only occurs if you opt in to `webcam` or `ndi` features:
```powershell
# Default build — no LLVM needed
cargo build --release --no-default-features

# With webcam (needs LLVM)
cargo build --release --features webcam
```

### "Library not loaded" errors

The build script (`build.rs`) auto-detects Syphon and NDI paths and embeds the correct rpaths in the binary. If you hit a `dyld` error:

1. **Syphon**: the script looks for `Syphon.framework` at `../syphon-rs/syphon-lib/`. If your layout differs, set `SYPHON_FRAMEWORK_DIR` to the directory containing the framework before building:
   ```bash
   SYPHON_FRAMEWORK_DIR=/path/to/syphon-rs/syphon-lib cargo build --release
   ```
2. **NDI**: the script checks `/usr/local/lib` and `/Library/NDI SDK for Apple/lib/macOS`. Install the NDI SDK and rebuild.
3. If you built the binary on another machine and copied it over, it will have the wrong rpath baked in — always rebuild from source on the target machine.

### No video input

Check that your camera/NDI source is available:
```bash
# List available cameras
cargo run -- --list-cameras

# List available NDI sources
cargo run -- --list-ndi
```

### Audio not responding

- Ensure audio input device is selected in Audio tab
- Check that audio routing is enabled
- Verify FFT visualization shows activity
- Try adjusting amplitude multiplier

### LFO not affecting output

- Ensure LFO is enabled and has a target parameter assigned
- Check that LFO amplitude is non-zero
- For tempo sync, ensure BPM is set (use tap tempo)
- Try disabling tempo sync and using free rate

### Web interface not connecting

- Check that web server is started (Web tab)
- Ensure firewall allows port 8080
- Try accessing via localhost first: `http://127.0.0.1:8080/rustjay`
- Check macOS Local Network permissions for the app

### Spout senders not appearing in the Input tab

- Click **Refresh Sources** — discovery runs on a background thread
- Verify the sending app (Resolume, OBS, etc.) is running and has an active Spout output
- Both apps must be on the same machine (Spout is local GPU sharing, not network)
- Check the app log for `[Spout] Discovery found N sender(s)` messages

### Spout output not visible in receiving apps

- Start the output from the **Output** tab → enter a sender name → **Start Spout Output**
- Verify in Resolume/OBS that the sender name matches what you entered
- The D3D11 shared texture uses the BGRA8 format; most receiving apps handle this automatically
- Check the app log for `[Spout] Registered sender` and `[Spout] Sender info written` messages

### Performance issues

- Run in release mode: `cargo run --release`
- Reduce internal resolution in settings
- Disable vsync for lower latency (may cause tearing)
- Lower audio FFT size if CPU-bound

## License

MIT License - See LICENSE file for details

## Credits

Built with:
- [wgpu](https://github.com/gfx-rs/wgpu) - Rust graphics API
- [imgui-rs](https://github.com/imgui-rs/imgui-rs) - ImGui bindings
- [grafton-ndi](https://crates.io/crates/grafton-ndi) - NDI bindings
- [nokhwa](https://github.com/l1npengtul/nokhwa) - Camera capture
- [realfft](https://github.com/HEnquist/realfft) - FFT library
- [axum](https://github.com/tokio-rs/axum) - Web framework
- [midir](https://github.com/Boddlnagg/midir) - MIDI library
