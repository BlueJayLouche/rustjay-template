# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [0.2.0] - 2026-03-14

### Added

#### LFO System
- 3 independent LFO banks with per-parameter assignment
- 5 waveforms: Sine, Triangle, Ramp, Saw, Square
- Tempo sync with beat divisions (1/16 to 8 beats)
- Phase offset control (0-360°, 0° aligns with beat)
- Target parameters: Hue Shift, Saturation, Brightness
- Real-time LFO window with collapsible sections

#### Audio Reactivity
- 8-band FFT routing matrix
- Route any FFT band to any HSB parameter
- Per-route attack/release smoothing
- Beat detection with automatic BPM estimation
- Tap tempo button in Audio tab

#### External Control
- MIDI input with CC mapping and learn system
- OSC server on UDP port 9000
- Web remote control via WebSocket (port 8080)
- Mobile-optimized web interface
- Auto-generated OSC addresses

#### Presets
- Quick slots: Shift+F1 through Shift+F8 for instant recall
- Named preset save/load/delete
- Import/export functionality
- Persistent storage in ~/.config/rustjay/

#### Settings Persistence
- Auto-save to ~/.config/rustjay/settings.json
- Window positions and sizes
- Device selections
- All parameter values
- LFO configurations
- Audio routing matrix
- MIDI mappings

### Fixed

- Fixed modulation feedback loop - base values now stable
- Fixed GUI sliders showing modulated values
- Fixed beat division selection in LFO GUI
- Fixed waveform change race condition
- Suppressed winit/tracing debug spam
- Fixed web server binding with localhost fallback

### Changed

- **Architecture**: Modulation now applied at render time, not update time
- **GUI**: Sliders now display base values (unaffected by modulation)
- **Data Flow**: Base values stored separately, modulations are additive offsets

### Documentation

- Complete README rewrite with new features
- Added AGENTS.md for AI assistant guidelines
- Added this CHANGELOG.md

## [0.1.0] - 2025

### Added

- Initial release
- Dual-window architecture (control + output)
- GPU-accelerated rendering with wgpu 25
- HSB color adjustments
- Webcam, NDI, and Syphon input
- NDI and Syphon output
- Basic audio analysis (8-band FFT)
- ImGui control interface

---

[Unreleased]: https://github.com/BlueJayLouche/rustjay-template/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/BlueJayLouche/rustjay-template/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/BlueJayLouche/rustjay-template/releases/tag/v0.1.0
