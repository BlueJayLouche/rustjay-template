# RustJay Template — Platform Porting Guide

This document is for the agent running on **Windows** (Spout) or **Linux** (V4L2).
The macOS host has already prepared the structural scaffolding. Your job is to fill in
the implementations on your platform.

---

## What has already been done (on macOS)

- `Cargo.toml` — `spout` and `v4l2` feature flags added; target-specific dependency
  sections stubbed out with `TODO` comments
- `src/core/state.rs` — `InputType::Spout` and `InputType::V4l2` variants added
- `src/input/mod.rs` — `InputCommand::StartSpout` / `StartV4l2` variants, stub fields
  and methods added to `InputManager`, discovery wired up
- `src/output/mod.rs` — `OutputCommand::StartSpout/StopSpout` / `StartV4l2/StopV4l2`
  variants, stub fields and methods added to `OutputManager`
- `src/input/spout_input.rs` — empty stub (Windows only)
- `src/output/spout_output.rs` — empty stub (Windows only)
- `src/output/v4l2_output.rs` — empty stub (Linux only)

All new code is behind `#[cfg(target_os = "windows")]` or `#[cfg(target_os = "linux")]`
so it does not affect the macOS build.

---

## The pattern to follow — study Syphon first

Before implementing anything, read these files to understand the established pattern:

```
src/input/syphon_input.rs      ← model for GPU-sharing input
src/output/syphon_output.rs    ← model for GPU-sharing output
src/input/mod.rs               ← how a new input backend is wired in
src/output/mod.rs              ← how a new output backend is wired in
src/gui/gui/tab_input.rs       ← how the GUI exposes input selection
src/gui/gui/tab_output.rs      ← how the GUI exposes output selection
```

Key things to notice:
- `InputManager` has one field per backend (`syphon_receiver: Option<...>`)
- `update()` polls each backend with a cfg guard
- `stop()` drops each backend with a cfg guard
- `begin_refresh_devices()` discovers available sources on a background thread
- GPU-based inputs (Syphon, Spout) get a zero-copy texture path; CPU-based ones
  go through `current_frame: Option<Vec<u8>>`
- Output uses `read_texture_bgra()` for CPU-path outputs (NDI, V4L2) and direct
  GPU texture passing for zero-copy outputs (Syphon, Spout)

---

## Windows — Spout

### What Spout is

Spout is the Windows equivalent of Syphon: GPU texture sharing between applications
via DirectX shared surfaces. Common in VJ software like Resolume, MadMapper, VDMX (Win).

### Step 1 — Find the right Rust crate

On Windows, search crates.io and GitHub for a Spout2 Rust wrapper:

```powershell
cargo search spout
```

Candidates to evaluate:
- Look for a crate wrapping the [Spout2 SDK](https://github.com/leadedge/Spout2)
- The SDK provides `SpoutSender` and `SpoutReceiver` C++ classes; a Rust wrapper
  will expose these via FFI
- If no maintained crate exists, the Spout2 SDK can be used via raw FFI with the
  `windows` crate — the SDK exposes a simple COM-like interface

Once identified, add it to `Cargo.toml`:
```toml
[target.'cfg(target_os = "windows")'.dependencies]
# Replace with the actual crate name and version:
# spout = { version = "x.y", optional = true }
```

And enable it in features:
```toml
[features]
spout = ["dep:spout"]   # replace dep name as needed
```

### Step 2 — Implement `src/input/spout_input.rs`

Model it on `src/input/syphon_input.rs`. You need:

```rust
pub struct SpoutSenderInfo {
    pub name: String,
    // Add other fields as the crate provides (width, height, etc.)
}

pub struct SpoutInputReceiver {
    // inner: Option<spout::Receiver>,  ← replace with actual type
    sender_name: Option<String>,
    resolution: (u32, u32),
}

impl SpoutInputReceiver {
    pub fn new() -> Self { ... }
    pub fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) { ... }
    pub fn connect(&mut self, sender_name: &str) -> anyhow::Result<()> { ... }
    pub fn disconnect(&mut self) { ... }
    pub fn try_receive_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool { ... }
    pub fn output_texture(&self) -> Option<&wgpu::Texture> { ... }
    pub fn resolution(&self) -> (u32, u32) { self.resolution }
}

pub struct SpoutDiscovery;
impl SpoutDiscovery {
    pub fn list_senders() -> Vec<SpoutSenderInfo> { ... }
}
```

### Step 3 — Implement `src/output/spout_output.rs`

Model it on `src/output/syphon_output.rs`. You need:

```rust
pub struct SpoutOutput {
    // inner: spout::Sender,
}

impl SpoutOutput {
    pub fn new(name: &str, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> anyhow::Result<Self> { ... }
    pub fn submit_frame(&mut self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> { ... }
}
```

### Step 4 — Fill in the stubs in `src/input/mod.rs`

Find the `// TODO (Windows): implement Spout` comment and replace:
```rust
pub fn start_spout(&mut self, sender_name: impl Into<String>) -> anyhow::Result<()> {
    let sender_name = sender_name.into();
    self.stop();
    let mut receiver = SpoutInputReceiver::new();
    // initialize with device/queue if needed
    receiver.connect(&sender_name)?;
    self.input_type = InputType::Spout;
    self.active = true;
    self.spout_receiver = Some(receiver);
    Ok(())
}
```

Also fill in the discovery call in `begin_refresh_devices()`.

### Step 5 — Fill in the stubs in `src/output/mod.rs`

Find the `// TODO (Windows): implement Spout` comment and fill in
`start_spout()`, `stop_spout()`, and the `submit_frame()` branch.

### Step 6 — Update the GUI

In `src/gui/gui/tab_input.rs`, find the `#[cfg(target_os = "windows")]` section and
add a Spout sender list and "Start Spout" button — following the Syphon section above
it as a model.

In `src/gui/gui/tab_output.rs`, add a Spout output name field and toggle — following
the Syphon section as a model.

### Step 7 — Build and test

```powershell
cargo build --release
cargo run --release
```

Open Resolume, OBS with Spout plugin, or any Spout-enabled app on the same machine
and verify the texture is shared correctly.

---

## Linux — V4L2

### What V4L2 loopback is

Video4Linux2 (V4L2) is the Linux video subsystem. With the `v4l2loopback` kernel module,
you can create virtual camera devices (e.g. `/dev/video10`) that other apps can read as
if they were real webcams. This is the Linux equivalent of Syphon/Spout for CPU-path sharing.

Note: **V4L2 input** (reading from real cameras) is likely already working on Linux via
`nokhwa` with the `input-native` feature (which maps to V4L2 on Linux). If `nokhwa`
doesn't detect cameras, try switching to the `input-v4l` feature in `Cargo.toml`.

This guide focuses on **V4L2 output** (writing to a loopback device).

### Step 1 — Install v4l2loopback

```bash
# Install the kernel module
sudo apt install v4l2loopback-dkms v4l2loopback-utils

# Load it with a virtual device
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="RustJay Output" exclusive_caps=1

# Verify it exists
v4l2-ctl --list-devices
```

To make it persistent across reboots:
```bash
echo "v4l2loopback" | sudo tee /etc/modules-load.d/v4l2loopback.conf
echo "options v4l2loopback devices=1 video_nr=10 card_label=RustJay exclusive_caps=1" \
  | sudo tee /etc/modprobe.d/v4l2loopback.conf
```

### Step 2 — Add the V4L2 crate

In `Cargo.toml`, find the `[target.'cfg(target_os = "linux")'.dependencies]` section
and uncomment / add:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
v4l = "0.14"
```

And in features:
```toml
[features]
v4l2 = ["dep:v4l"]
```

### Step 3 — Implement `src/output/v4l2_output.rs`

V4L2 output is CPU-path — `read_texture_bgra()` in `OutputManager` handles the GPU
readback. You then write those bytes to the loopback device:

```rust
use v4l::prelude::*;
use v4l::video::Output;

pub struct V4l2LoopbackOutput {
    device: Device,
    width: u32,
    height: u32,
}

impl V4l2LoopbackOutput {
    pub fn new(device_path: &str, width: u32, height: u32) -> anyhow::Result<Self> {
        let device = Device::new(device_path)?; // e.g. "/dev/video10"
        // Configure the device format (BGRA / BGR3 etc — check what v4l2loopback accepts)
        // Most apps expect YUV or BGR; you may need a BGRA→BGR3 conversion here
        Ok(Self { device, width, height })
    }

    pub fn send_frame(&mut self, bgra_data: &[u8]) -> anyhow::Result<()> {
        // Write BGRA (or converted) bytes to the V4L2 device
        use std::io::Write;
        self.device.write_all(bgra_data)?;
        Ok(())
    }
}
```

Note on format: many apps that read V4L2 expect `YUYV` or `BGR3`. If apps don't see
the stream, convert from BGRA before writing. The `image` crate can help.

### Step 4 — Fill in the stubs in `src/output/mod.rs`

Find the `// TODO (Linux): implement V4L2` comment:

```rust
pub fn start_v4l2(&mut self, device_path: impl Into<String>, width: u32, height: u32) -> anyhow::Result<()> {
    let path = device_path.into();
    let output = V4l2LoopbackOutput::new(&path, width, height)?;
    self.v4l2_output = Some(output);
    log::info!("V4L2 output started on {}", path);
    Ok(())
}
```

And in `submit_frame()`, add the V4L2 branch alongside the NDI branch (both use the
CPU readback path):

```rust
#[cfg(target_os = "linux")]
if let Some(ref mut v4l2) = self.v4l2_output {
    if let Some(data) = self.read_texture_bgra(texture, device, queue) {
        if let Err(e) = v4l2.send_frame(&data) {
            log::error!("V4L2 output error: {}", e);
        }
    }
}
```

### Step 5 — Update the GUI

In `src/gui/gui/tab_output.rs`, find the `#[cfg(target_os = "linux")]` section and add:
- A text field for the device path (default: `/dev/video10`)
- A "Start V4L2 Output" / "Stop" toggle button
- Status indicator

### Step 6 — Test

```bash
cargo build --release
cargo run --release
# In another terminal, verify the stream:
ffplay /dev/video10
# Or open OBS and add a Video Capture Device source pointing to "RustJay Output"
```

---

## After implementing — push your changes

```bash
git add -A
git commit -m "feat: implement [Spout/V4L2] input and output"
git push
```

The macOS machine will then pull your changes and integrate them into the other projects
(`rustjay-waaaves`, `rustjay-delta`, `rustjay-mapper`) which follow the same pattern.

---

## Questions / Decisions to document

As you implement, note any decisions that weren't obvious here — add them to this file
under a `## Notes` section so the macOS host knows what was chosen and why. For example:
- Which Spout crate was used and why
- Whether format conversion was needed for V4L2
- Any platform-specific quirks
