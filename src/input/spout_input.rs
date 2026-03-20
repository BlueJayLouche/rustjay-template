//! # Spout Input (Windows)
//!
//! GPU texture sharing input via Spout2 (DirectX shared surfaces).
//! This is the Windows equivalent of Syphon input.
//!
//! ## Implementation
//!
//! Spout senders register themselves in two Windows named shared-memory mappings:
//!   - `"SpoutSenderNames"` — global list (DWORD count + char[256] names)
//!   - `"<sender_name>"`    — per-sender info (SpoutSenderInfoRaw struct)
//!
//! The per-sender info contains the DXGI `GetSharedHandle` value. We open the
//! shared D3D11 texture with `ID3D11Device::OpenSharedResource`, copy to a
//! staging texture each frame, and map it to read BGRA pixels into the
//! `InputManager` CPU buffer.
//!
//! This is NOT zero-copy (unlike Syphon on macOS). Zero-copy would require
//! D3D11↔D3D12 texture interop; that is deferred.

#![cfg(target_os = "windows")]

use std::ffi::CStr;

use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::Win32::System::Memory::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingA, UnmapViewOfFile};

/// SpoutSenderInfo as stored in per-sender named shared memory.
///
/// Layout matches Spout2 SDK `SpoutSenderNames.h` on x64 Windows:
/// - offset  0: width  (u32)
/// - offset  4: height (u32)
/// - offset  8: dwFormat (u32) — DXGI_FORMAT_B8G8R8A8_UNORM = 87
/// - offset 12: padding (u32) to align HANDLE to 8 bytes
/// - offset 16: shareHandle (usize / HANDLE)
/// - offset 24: NameCount (u32)
/// - offset 28: padding (u32) for 32-byte struct size
#[repr(C)]
struct SpoutSenderInfoRaw {
    width: u32,
    height: u32,
    dw_format: u32,
    _pad: u32,
    share_handle: usize,
    name_count: u32,
    _pad2: u32,
}

/// Information about an available Spout sender
#[derive(Debug, Clone)]
pub struct SpoutSenderInfo {
    /// Sender name as registered in `SpoutSenderNames` shared memory
    pub name: String,
    /// Width of the shared texture (0 = unavailable)
    pub width: u32,
    /// Height of the shared texture (0 = unavailable)
    pub height: u32,
}

/// Discovers active Spout senders on this machine by reading the
/// `SpoutSenderNames` Windows named shared-memory mapping.
pub struct SpoutDiscovery;

impl SpoutDiscovery {
    /// Return a list of all active Spout senders.
    pub fn list_senders() -> Vec<SpoutSenderInfo> {
        unsafe {
            let map_name = windows::core::s!("SpoutSenderNames");
            let Ok(hmap) = OpenFileMappingA(FILE_MAP_READ.0, false, map_name) else {
                // Mapping doesn't exist → no active senders
                return Vec::new();
            };

            let view = MapViewOfFile(hmap, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                CloseHandle(hmap).ok();
                return Vec::new();
            }

            let base = view.Value as *const u8;
            let count = *(base as *const u32) as usize;
            log::debug!("[Spout] Discovery: {} sender(s) in SpoutSenderNames", count);

            let mut senders = Vec::with_capacity(count);
            for i in 0..count {
                let name_ptr = base.add(4 + i * 256) as *const i8;
                let name = CStr::from_ptr(name_ptr).to_string_lossy().into_owned();
                if name.is_empty() {
                    continue;
                }
                let (width, height) = read_sender_dimensions(&name);
                log::debug!("[Spout]   sender[{}]: '{}' {}x{}", i, name, width, height);
                senders.push(SpoutSenderInfo { name, width, height });
            }

            UnmapViewOfFile(view).ok();
            CloseHandle(hmap).ok();
            senders
        }
    }
}

/// Read width/height from the per-sender named shared memory block.
unsafe fn read_sender_dimensions(name: &str) -> (u32, u32) {
    let Ok(cname) = std::ffi::CString::new(name) else {
        return (0, 0);
    };
    let Ok(hmap) = OpenFileMappingA(
        FILE_MAP_READ.0,
        false,
        windows::core::PCSTR(cname.as_ptr() as *const u8),
    ) else {
        return (0, 0);
    };

    let view = MapViewOfFile(hmap, FILE_MAP_READ, 0, 0, 0);
    let result = if !view.Value.is_null() {
        let info = &*(view.Value as *const SpoutSenderInfoRaw);
        let dims = (info.width, info.height);
        UnmapViewOfFile(view).ok();
        dims
    } else {
        (0, 0)
    };
    CloseHandle(hmap).ok();
    result
}

/// Read share handle and dimensions from a sender's named shared-memory block.
unsafe fn read_sender_info(name: &str) -> anyhow::Result<(HANDLE, u32, u32)> {
    let cname = std::ffi::CString::new(name)?;
    let hmap = OpenFileMappingA(
        FILE_MAP_READ.0,
        false,
        windows::core::PCSTR(cname.as_ptr() as *const u8),
    )
    .map_err(|_| anyhow::anyhow!("[Spout] sender '{}' not in shared memory", name))?;

    let view = MapViewOfFile(hmap, FILE_MAP_READ, 0, 0, 0);
    if view.Value.is_null() {
        CloseHandle(hmap).ok();
        return Err(anyhow::anyhow!(
            "[Spout] MapViewOfFile failed for sender '{}'",
            name
        ));
    }

    let info = &*(view.Value as *const SpoutSenderInfoRaw);
    let handle = HANDLE(info.share_handle as isize);
    let width = info.width;
    let height = info.height;

    UnmapViewOfFile(view).ok();
    CloseHandle(hmap).ok();
    Ok((handle, width, height))
}

/// Receives frames from a Spout sender as CPU pixel bytes → wgpu texture.
///
/// Opens the sender's D3D11 shared texture via its DXGI handle, copies to a
/// staging texture each frame, and exposes BGRA bytes via [`take_pixels`].
pub struct SpoutInputReceiver {
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    /// Name of the connected sender (None = disconnected)
    sender_name: Option<String>,
    /// The shared D3D11 texture from the sender
    shared_texture: Option<ID3D11Texture2D>,
    /// CPU-readable staging copy
    staging_texture: Option<ID3D11Texture2D>,
    /// Current resolution of the shared texture
    resolution: (u32, u32),
    /// BGRA pixel buffer filled by `try_receive_texture()`
    pixel_buffer: Vec<u8>,
}

impl SpoutInputReceiver {
    /// Create an unconnected receiver and initialise the D3D11 device.
    pub fn new() -> Self {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("[Spout] SpoutInputReceiver: D3D11CreateDevice failed");

            log::info!("[Spout] SpoutInputReceiver: D3D11 device created");
            Self {
                d3d_device: device.expect("D3D11 device"),
                d3d_context: context.expect("D3D11 context"),
                sender_name: None,
                shared_texture: None,
                staging_texture: None,
                resolution: (0, 0),
                pixel_buffer: Vec::new(),
            }
        }
    }

    /// Connect to the named Spout sender and open its shared D3D11 texture.
    pub fn connect(&mut self, sender_name: &str) -> anyhow::Result<()> {
        self.disconnect();
        self.sender_name = Some(sender_name.to_string());
        self.open_shared_texture()?;
        log::info!("[Spout] Connected to sender: {}", sender_name);
        Ok(())
    }

    /// Disconnect from the current sender and release D3D11 resources.
    pub fn disconnect(&mut self) {
        self.shared_texture = None;
        self.staging_texture = None;
        self.resolution = (0, 0);
        self.pixel_buffer.clear();
        if let Some(ref name) = self.sender_name {
            log::info!("[Spout] Disconnected from '{}'", name);
        }
        self.sender_name = None;
    }

    /// Open (or re-open) the shared D3D11 texture for the connected sender.
    fn open_shared_texture(&mut self) -> anyhow::Result<()> {
        let sender_name = self
            .sender_name
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("[Spout] not connected to any sender"))?
            .to_string();

        unsafe {
            let (handle, width, height) = read_sender_info(&sender_name)?;

            if width == 0 || height == 0 {
                return Err(anyhow::anyhow!(
                    "[Spout] sender '{}' has zero dimensions",
                    sender_name
                ));
            }
            if handle.0 == 0 {
                return Err(anyhow::anyhow!(
                    "[Spout] sender '{}' has null share handle",
                    sender_name
                ));
            }

            // Open the shared texture on our D3D11 device
            let mut shared_tex: Option<ID3D11Texture2D> = None;
            self.d3d_device
                .OpenSharedResource(handle, &mut shared_tex)?;
            let shared_tex = shared_tex.ok_or_else(|| {
                anyhow::anyhow!("[Spout] OpenSharedResource returned None")
            })?;

            // Create a CPU-readable staging texture of the same size
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut staging = None;
            self.d3d_device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
            let staging = staging.ok_or_else(|| {
                anyhow::anyhow!("[Spout] CreateTexture2D (staging) returned None")
            })?;

            log::info!(
                "[Spout] Opened shared texture {}x{} from '{}' (handle={:?})",
                width,
                height,
                sender_name,
                handle
            );

            self.shared_texture = Some(shared_tex);
            self.staging_texture = Some(staging);
            self.resolution = (width, height);
        }
        Ok(())
    }

    /// Poll for a new frame.
    ///
    /// Copies the sender's shared D3D11 texture to the staging buffer and
    /// reads the BGRA pixels. Returns `true` when new pixels are ready.
    /// Call [`take_pixels`](Self::take_pixels) to move them out.
    pub fn try_receive_texture(&mut self) -> bool {
        if self.sender_name.is_none() {
            return false;
        }

        // (Re-)open if not connected yet or sender restarted
        if self.shared_texture.is_none() {
            if self.open_shared_texture().is_err() {
                return false;
            }
        }

        let (w, h) = self.resolution;
        if w == 0 || h == 0 {
            return false;
        }

        unsafe {
            let shared_tex = match self.shared_texture.as_ref() {
                Some(t) => t,
                None => return false,
            };
            let staging_tex = match self.staging_texture.as_ref() {
                Some(t) => t,
                None => return false,
            };

            // Copy under keyed mutex if present (sender uses key=0)
            match shared_tex.cast::<IDXGIKeyedMutex>() {
                Ok(keyed_mutex) => {
                    if keyed_mutex.AcquireSync(0, 16).is_err() {
                        return false; // sender busy or timed out
                    }
                    self.d3d_context.CopyResource(staging_tex, shared_tex);
                    keyed_mutex.ReleaseSync(0).ok();
                }
                Err(_) => {
                    // No keyed mutex — plain shared texture, copy directly
                    self.d3d_context.CopyResource(staging_tex, shared_tex);
                }
            }

            // Map staging texture and read BGRA bytes row by row
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if self
                .d3d_context
                .Map(staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .is_err()
            {
                return false;
            }

            let needed = (w * h * 4) as usize;
            if self.pixel_buffer.len() != needed {
                self.pixel_buffer.resize(needed, 0);
            }

            let src = mapped.pData as *const u8;
            let row_pitch = mapped.RowPitch as usize;
            let dst_row_bytes = (w * 4) as usize;
            for row in 0..h as usize {
                let src_row =
                    std::slice::from_raw_parts(src.add(row * row_pitch), dst_row_bytes);
                self.pixel_buffer[row * dst_row_bytes..(row + 1) * dst_row_bytes]
                    .copy_from_slice(src_row);
            }

            self.d3d_context.Unmap(staging_tex, 0);
        }

        true
    }

    /// Move the pixel buffer out of the receiver.
    ///
    /// Returns `Some(Vec<u8>)` (BGRA, row-major) when a frame was received.
    /// Leaves the internal buffer empty until the next `try_receive_texture()`.
    pub fn take_pixels(&mut self) -> Option<Vec<u8>> {
        if self.pixel_buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pixel_buffer))
        }
    }

    /// The output texture — always `None` (CPU path, use `take_pixels()` instead).
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        None
    }

    /// Current resolution of the shared texture
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }
}

impl Default for SpoutInputReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SpoutInputReceiver {
    fn drop(&mut self) {
        self.disconnect();
    }
}
