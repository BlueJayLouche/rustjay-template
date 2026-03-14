# Agent Guidelines for RustJay Template

This file provides context and guidelines for AI assistants working on the RustJay Template project.

## Project Overview

RustJay Template is a high-performance video processing application with a complex modular architecture. It's essentially a simplified VJ (Visual Jockey) tool for live video mixing and effects.

## Architecture Principles

### 1. Separation of Concerns

The most important architectural pattern is the **separation of base values from modulation**:

- **Base Values**: Stored in `audio_routing.base_*` - these are the "user set" values
- **Modulation**: Applied at render time as additive offsets
- **Final Values**: Computed each frame: `final = base + lfo_mod + audio_mod`

**NEVER** write modulated values back to base values - this causes feedback loops.

### 2. Frame Update Flow

```
┌─────────────────────────────────────────────────────────────┐
│                         Frame Update                         │
├─────────────────────────────────────────────────────────────┤
│  1. Process Commands (input/output/audio/MIDI/OSC/web)      │
│  2. Update Audio (FFT analysis, beat detection)             │
│  3. Update LFO Phases (only update phase accumulators)      │
│  4. Update MIDI/OSC (process incoming messages)             │
│  5. Render (composite base + modulations)                   │
└─────────────────────────────────────────────────────────────┘
```

### 3. State Management

- `SharedState` is the single source of truth, wrapped in `Arc<Mutex<>>`
- GUI reads from base values to display sliders
- GUI writes to base values when user interacts
- Modulation systems only update their internal state (phases, smoothed values)
- Renderer composites everything at the last moment

## Key Modules

### Core (`src/core/`)

- **state.rs**: SharedState definition - the central state container
- **lfo.rs**: LFO engine - waveform generation, phase accumulation
- **vertex.rs**: GPU vertex data structures

### Audio (`src/audio/`)

- **mod.rs**: Audio capture, FFT analysis, beat detection
- **routing.rs**: Audio→parameter routing matrix with attack/release smoothing

### Engine (`src/engine/`)

- **renderer.rs**: Main wgpu renderer - this is where modulation is applied!
- **texture.rs**: Texture management

### GUI (`src/gui/`)

- **gui.rs**: ImGui interface builder - reads from base values
- **renderer.rs**: ImGui wgpu integration

### Control Systems

- **midi/mod.rs**: MIDI input with learn system
- **osc/mod.rs**: OSC server (UDP port 9000)
- **web/mod.rs**: WebSocket server + embedded HTML interface
- **presets/mod.rs**: Save/load system
- **config/mod.rs**: Settings persistence

## Common Patterns

### Adding a New Modulation Target

1. Add to appropriate params struct (if creating new parameter system)
2. Update the modulation application in the render step
3. Add GUI controls (reading from base, writing to base)
4. Update web/OSC/MIDI handlers to handle the new parameter

### Fixing "Values Drifting" Issues

If parameter values drift over time or modulations compound:

1. Check that GUI reads from `audio_routing.base_*` not `hsb_params`
2. Check that modulation is not writing back to base values
3. Ensure render step starts with base values and applies modulations additively

### Adding New Audio Routing Targets

1. Add to `ModulationTarget` enum in `src/audio/routing.rs`
2. Update `apply_to_hsb()` or add new apply function
3. Update GUI to allow selecting the new target
4. Update modulation range (check if it's 0-1, -1 to 1, degrees, etc.)

## Important Constants

### LFO
- Beat divisions: 1/16, 1/8, 1/4, 1/2, 1, 2, 4, 8 beats
- Waveforms: Sine(0), Triangle(1), Ramp(2), Saw(3), Square(4)
- Default rate: 0.15 (free mode) or 2 (1/4 beat in tempo sync)
- Amplitude range: -1.0 to 1.0

### Audio
- FFT bands: 8 bands from 20Hz to 16kHz
- Attack/release: 0.0-1.0 (0 = instant, 1 = very slow)
- Modulation clamp: -2.0 to 2.0 (summed across all routes)

### HSB Parameters
- Hue: -180° to 180°
- Saturation: 0.0 to 2.0 (1.0 = no change)
- Brightness: 0.0 to 2.0 (1.0 = no change)

## Common Pitfalls

### 1. Variable Shadowing in GUI

```rust
// WRONG - division_idx shadowed
let division_idx = bank.division;
if ui.combo(..., &mut division_idx) && division_idx != division_idx { ... }

// RIGHT - compare against original
let current_division = bank.division;
let mut division_idx = current_division;
if ui.combo(..., &mut division_idx) && division_idx != current_division { ... }
```

### 2. Writing Modulated Values to Base

```rust
// WRONG - creates feedback loop
state.hsb_params.hue = base_hue + modulation;
state.audio_routing.base_hue = state.hsb_params.hue; // NO!

// RIGHT - keep them separate
// (GUI writes to base, render reads base + applies modulation)
```

### 3. Borrow Issues with Mutex

```rust
// WRONG - holding lock while doing UI
let state = self.shared_state.lock().unwrap();
if ui.button("Click") { state.value = 1; } // Can't mutably borrow

// RIGHT - get values first, then update
let value = { let state = self.shared_state.lock().unwrap(); state.value };
if ui.button("Click") { 
    let mut state = self.shared_state.lock().unwrap();
    state.value = 1;
}
```

## Testing Checklist

When adding new features, verify:

- [ ] GUI displays correct base values (not modulated)
- [ ] GUI updates base values correctly
- [ ] Modulation works when enabled
- [ ] Values return to base when modulation disabled
- [ ] No drift/compounding over time
- [ ] Presets save/load correctly
- [ ] Web interface reflects changes
- [ ] MIDI/OSC can control the parameter

## External Resources

- Repository: https://github.com/BlueJayLouche/rustjay-template
- NDI SDK: https://ndi.tv
- wgpu docs: https://docs.rs/wgpu/0.25.0/wgpu/
- ImGui wgpu: https://github.com/Yatekii/imgui-wgpu-rs

## Build Notes

- Always build with `--release` for testing (debug is very slow)
- On macOS, the build script handles framework linking automatically
- The web UI is embedded at compile time from `src/web/ui.html`

## Code Style

- Use `log::info!`, `log::warn!`, `log::error!` for logging (not println)
- Prefer `f32` over `f64` for GPU compatibility
- Use glam types (Vec3, Vec4) for shader-bound data
- Keep modulation logic in the render step, not in update loops
