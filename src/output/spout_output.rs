//! # Spout Output (Windows)
//!
//! GPU texture sharing output via Spout2 using the `windows` crate.
//!
//! ## Architecture
//!
//! There is no maintained Rust Spout2 *sender* crate, so we implement the
//! Spout sender protocol directly:
//!
//! 1. Create a standalone D3D11 device (wgpu on Windows uses D3D12, so we
//!    maintain a separate D3D11 device solely for Spout sharing).
//! 2. Create a D3D11 shared texture with `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`.
//! 3. Register the sender in two Windows named shared-memory mappings that
//!    Spout uses as its sender registry (`SpoutSenderNames` + per-sender info).
//! 4. Each frame: read back the wgpu render target to CPU bytes, then
//!    `UpdateSubresource` into the D3D11 shared texture under the keyed mutex.
//!
//! Receiving apps (Resolume, OBS Spout plugin, etc.) discover the sender via
//! the shared memory registry and open the texture by its DXGI shared handle.

#![cfg(target_os = "windows")]

use std::sync::Arc;

use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, INVALID_HANDLE_VALUE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_CREATE_DEVICE_FLAG, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGIKeyedMutex, IDXGIResource};
use windows::Win32::System::Memory::{
    CreateFileMappingA, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
};

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
struct SpoutSenderInfo {
    width: u32,
    height: u32,
    dw_format: u32,
    _pad: u32,
    share_handle: usize,
    name_count: u32,
    _pad2: u32,
}

/// Spout sender — shares the wgpu render target with other apps on this machine.
pub struct SpoutOutput {
    sender_name: String,
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    shared_texture: Option<ID3D11Texture2D>,
    share_handle: HANDLE,
    width: u32,
    height: u32,
}

impl SpoutOutput {
    /// Create a new Spout sender with the given name.
    ///
    /// Initialises a standalone D3D11 device for texture sharing.
    pub fn new(
        name: &str,
        _device: Arc<wgpu::Device>,
        _queue: Arc<wgpu::Queue>,
    ) -> anyhow::Result<Self> {
        unsafe {
            let mut d3d_device = None;
            let mut d3d_context = None;

            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None, // default feature levels
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                Some(&mut d3d_context),
            )?;

            let d3d_device = d3d_device
                .ok_or_else(|| anyhow::anyhow!("[Spout] D3D11CreateDevice returned no device"))?;
            let d3d_context = d3d_context.ok_or_else(|| {
                anyhow::anyhow!("[Spout] D3D11CreateDevice returned no context")
            })?;

            log::info!("[Spout] D3D11 device created for sender '{}'", name);

            Ok(Self {
                sender_name: name.to_string(),
                d3d_device,
                d3d_context,
                shared_texture: None,
                share_handle: HANDLE::default(),
                width: 0,
                height: 0,
            })
        }
    }

    /// Share the wgpu render target with connected Spout receivers.
    pub fn submit_frame(
        &mut self,
        texture: &wgpu::Texture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        let width = texture.width();
        let height = texture.height();

        // (Re-)create the shared D3D11 texture when dimensions change
        if self.shared_texture.is_none() || self.width != width || self.height != height {
            self.shared_texture = None;
            self.create_shared_texture(width, height)?;
        }

        // Read wgpu render target back to CPU bytes (BGRA)
        let bytes = Self::read_texture_bgra(texture, device, queue)
            .ok_or_else(|| anyhow::anyhow!("[Spout] GPU readback failed"))?;

        unsafe {
            let d3d_tex = self.shared_texture.as_ref().unwrap();
            let keyed_mutex: IDXGIKeyedMutex = d3d_tex.cast()?;

            // Acquire mutex with sender key (0), 16 ms timeout (~1 frame at 60 Hz)
            keyed_mutex.AcquireSync(0, 16)?;

            self.d3d_context.UpdateSubresource(
                d3d_tex,
                0,    // subresource
                None, // full extent
                bytes.as_ptr() as *const _,
                width * 4, // row pitch (BGRA = 4 bytes/pixel)
                0,         // depth pitch (not a 3D texture)
            );

            // Release with sender key (0) so receiver can acquire with key 0
            keyed_mutex.ReleaseSync(0)?;
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    fn create_shared_texture(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
            };

            let mut tex = None;
            self.d3d_device.CreateTexture2D(&desc, None, Some(&mut tex))?;
            let tex: ID3D11Texture2D =
                tex.ok_or_else(|| anyhow::anyhow!("[Spout] CreateTexture2D returned None"))?;

            // Get the DXGI shared handle (legacy GetSharedHandle — used by Spout2)
            let dxgi_resource: IDXGIResource = tex.cast()?;
            let handle = dxgi_resource.GetSharedHandle()?;

            log::info!(
                "[Spout] Shared texture {}x{} created, handle={:?}",
                width,
                height,
                handle
            );

            self.share_handle = handle;
            self.shared_texture = Some(tex);
            self.width = width;
            self.height = height;

            self.register_spout_sender(width, height, handle)?;
        }
        Ok(())
    }

    /// Register this sender in the two Spout shared-memory maps that Spout
    /// uses to discover senders:
    ///   - `"SpoutSenderNames"` — global list of active sender names
    ///   - `"<sender_name>"`    — per-sender info (dimensions + share handle)
    unsafe fn register_spout_sender(
        &self,
        width: u32,
        height: u32,
        handle: HANDLE,
    ) -> anyhow::Result<()> {
        // ── Global sender name list ───────────────────────────────────────────
        // Layout: DWORD count @ offset 0, then up to 256 × char[256] names.
        let map_name = windows::core::s!("SpoutSenderNames");
        let hmap = CreateFileMappingA(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            4096,
            map_name,
        )?;

        let view = MapViewOfFile(hmap, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if view.Value.is_null() {
            CloseHandle(hmap).ok();
            return Err(anyhow::anyhow!(
                "[Spout] MapViewOfFile failed for SpoutSenderNames"
            ));
        }

        {
            let base = view.Value as *mut u8;
            let count_ptr = base as *mut u32;
            let count = *count_ptr;
            let name_bytes = self.sender_name.as_bytes();
            let mut already_present = false;

            // Check whether our name is already in the list
            for i in 0..count {
                let slot = base.add(4 + i as usize * 256);
                let slot_end = slot.add(256);
                // Find the null terminator
                let mut len = 0usize;
                while len < 256 {
                    if *slot.add(len) == 0 {
                        break;
                    }
                    len += 1;
                }
                if len == name_bytes.len()
                    && std::slice::from_raw_parts(slot, len) == name_bytes
                {
                    already_present = true;
                    break;
                }
                let _ = slot_end; // suppress warning
            }

            if !already_present && (count as usize) < 256 {
                let slot = base.add(4 + count as usize * 256);
                let copy_len = name_bytes.len().min(255);
                std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), slot, copy_len);
                *slot.add(copy_len) = 0; // null-terminate
                *count_ptr = count + 1;
                log::info!(
                    "[Spout] Registered '{}' in SpoutSenderNames (slot {})",
                    self.sender_name,
                    count
                );
            }
        }

        UnmapViewOfFile(view).ok();
        CloseHandle(hmap).ok();

        // ── Per-sender info block ─────────────────────────────────────────────
        let sender_cstr = std::ffi::CString::new(self.sender_name.as_str())
            .map_err(|e| anyhow::anyhow!("[Spout] invalid sender name: {}", e))?;

        let hmap2 = CreateFileMappingA(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            std::mem::size_of::<SpoutSenderInfo>() as u32,
            windows::core::PCSTR(sender_cstr.as_ptr() as *const u8),
        )?;

        let view2 = MapViewOfFile(hmap2, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if view2.Value.is_null() {
            CloseHandle(hmap2).ok();
            return Err(anyhow::anyhow!(
                "[Spout] MapViewOfFile failed for sender info '{}'",
                self.sender_name
            ));
        }

        let info_ptr = view2.Value as *mut SpoutSenderInfo;
        *info_ptr = SpoutSenderInfo {
            width,
            height,
            dw_format: 87, // DXGI_FORMAT_B8G8R8A8_UNORM
            _pad: 0,
            share_handle: handle.0 as usize,
            name_count: 1,
            _pad2: 0,
        };

        UnmapViewOfFile(view2).ok();
        CloseHandle(hmap2).ok();

        log::info!(
            "[Spout] Sender info written for '{}' {}x{}",
            self.sender_name,
            width,
            height
        );
        Ok(())
    }

    /// Remove this sender from the global `SpoutSenderNames` list.
    unsafe fn unregister_spout_sender(&self) {
        let map_name = windows::core::s!("SpoutSenderNames");
        let Ok(hmap) = CreateFileMappingA(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            4096,
            map_name,
        ) else {
            return;
        };

        let view = MapViewOfFile(hmap, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if !view.Value.is_null() {
            let base = view.Value as *mut u8;
            let count_ptr = base as *mut u32;
            let count = *count_ptr;
            let name_bytes = self.sender_name.as_bytes();

            for i in 0..count {
                let slot = base.add(4 + i as usize * 256);
                let mut len = 0usize;
                while len < 256 {
                    if *slot.add(len) == 0 {
                        break;
                    }
                    len += 1;
                }
                if len == name_bytes.len()
                    && std::slice::from_raw_parts(slot, len) == name_bytes
                {
                    // Compact: shift remaining entries down one slot
                    let remaining = count - i - 1;
                    if remaining > 0 {
                        std::ptr::copy(
                            base.add(4 + (i as usize + 1) * 256),
                            slot,
                            remaining as usize * 256,
                        );
                    }
                    // Zero the vacated last slot
                    let last_slot = base.add(4 + (count - 1) as usize * 256);
                    std::ptr::write_bytes(last_slot, 0, 256);
                    *count_ptr = count - 1;
                    log::info!("[Spout] Unregistered '{}' from SpoutSenderNames", self.sender_name);
                    break;
                }
            }

            UnmapViewOfFile(view).ok();
        }
        CloseHandle(hmap).ok();
    }

    /// Identical to `OutputManager::read_texture_bgra` — reads a wgpu texture
    /// back to CPU as a BGRA byte vec.
    fn read_texture_bgra(
        texture: &wgpu::Texture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<Vec<u8>> {
        let width = texture.width();
        let height = texture.height();
        let bytes_per_row = width * 4;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Spout Output Readback"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Spout Output Readback Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r.is_ok());
        });
        device.poll(wgpu::PollType::Wait).ok();

        if rx.recv().ok()? {
            let data = slice.get_mapped_range();
            let bytes = data.to_vec();
            drop(data);
            staging.unmap();
            Some(bytes)
        } else {
            None
        }
    }
}

impl Drop for SpoutOutput {
    fn drop(&mut self) {
        unsafe {
            self.unregister_spout_sender();
        }
        log::info!("[Spout] Sender '{}' dropped", self.sender_name);
    }
}
