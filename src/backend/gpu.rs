/// Per-GPU state: DRM device, GBM allocator, GlesRenderer, one DrmCompositor per output.
///
/// Call `GpuData::init()` once per discovered GPU, then `render_all()` every frame tick.
use std::collections::HashSet;
use std::os::unix::io::{AsFd, OwnedFd};
use std::path::Path;

use smithay::{
    backend::{
        allocator::{
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Format as DrmFormat, Fourcc as DrmFourcc,
        },
        drm::{
            compositor::{DrmCompositor, FrameFlags},
            exporter::gbm::GbmFramebufferExporter,
            DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmNode,
        },
        egl::{EGLContext, EGLDisplay},
        renderer::{
            element::{
                texture::{TextureBuffer, TextureRenderElement},
                Kind,
            },
            gles::{GlesRenderer, GlesTexture},
            Color32F, ImportDma, ImportMem,
        },
        session::Session,
    },
    output::{Mode as SmithayMode, Output, PhysicalProperties, Subpixel},
    reexports::{
        drm::control::{connector, crtc, Device as ControlDevice, ModeTypeFlags},
        rustix::fs::OFlags,
    },
    utils::{DeviceFd, Scale, Transform},
};
use tracing::{error, info, warn};

use crate::render::{
    cursor,
    PancakeElements,
};
use crate::state::PancakeState;

// ── Type alias for our DrmCompositor ─────────────────────────────────────────

pub type PancakeCompositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

// ── Per-output state ──────────────────────────────────────────────────────────

pub struct OutputState {
    pub output: Output,
    pub compositor: PancakeCompositor,
    pub size: (u16, u16),
    pub frame_seq: u64,
}

// ── Per-GPU state ─────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct GpuData {
    pub drm: DrmDevice,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub renderer: GlesRenderer,
    pub outputs: Vec<OutputState>,
    pub node: DrmNode,
    /// Cursor texture buffer — passed as a render element into render_frame each frame.
    pub cursor_buffer: Option<TextureBuffer<GlesTexture>>,
    /// Hotspot offset of the cursor image in logical pixels.
    pub cursor_hotspot: (u32, u32),
}

impl GpuData {
    /// Open a GPU at `path`, initialise DRM + GBM + EGL + GLES, then create one
    /// `DrmCompositor` per connected connector.
    pub fn init<S>(
        session: &mut S,
        path: &Path,
        _space: &smithay::desktop::Space<smithay::desktop::Window>,
    ) -> Result<(GpuData, DrmDeviceNotifier), Box<dyn std::error::Error>>
    where
        S: Session,
        S::Error: std::error::Error + Send + Sync + 'static,
    {
        // ── Open device via libseat (obtains DRM master) ──────────────────────
        let fd: OwnedFd = session.open(path, OFlags::empty())?;
        let device_fd = DeviceFd::from(fd);
        let drm_fd = DrmDeviceFd::new(device_fd);

        let drm_node = DrmNode::from_file(drm_fd.as_fd())
            .or_else(|_| DrmNode::from_path(path))
            .map_err(|e| format!("failed to get drm node for {path:?}: {e}"))?;

        info!("Initialising GPU {:?} (node {:?})", path, drm_node);

        // ── DRM device ───────────────────────────────────────────────────────
        let (mut drm, notifier) = DrmDevice::new(drm_fd.clone(), true)?;

        // ── GBM device ───────────────────────────────────────────────────────
        let gbm: GbmDevice<DrmDeviceFd> = GbmDevice::new(drm_fd.clone())?;

        // ── EGL + GLES ───────────────────────────────────────────────────────
        let egl_display = unsafe { EGLDisplay::new(gbm.clone())? };
        let egl_ctx = EGLContext::new(&egl_display)?;
        let mut renderer = unsafe { GlesRenderer::new(egl_ctx)? };

        // ── Enumerate connectors ─────────────────────────────────────────────
        let resources = drm.resource_handles()?;
        let mut used_crtcs: HashSet<crtc::Handle> = HashSet::new();
        let mut outputs: Vec<OutputState> = Vec::new();

        for &conn_handle in resources.connectors() {
            let conn = match drm.get_connector(conn_handle, true) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to get connector info: {e}");
                    continue;
                }
            };

            if conn.state() != connector::State::Connected {
                info!(
                    "Connector {:?}-{} is {:?}; skipping",
                    conn.interface(),
                    conn.interface_id(),
                    conn.state()
                );
                continue;
            }

            // Prefer PREFERRED flag; fall back to highest pixel count.
            let drm_mode = conn
                .modes()
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
                .or_else(|| {
                    conn.modes()
                        .iter()
                        .max_by_key(|m| m.size().0 as u32 * m.size().1 as u32)
                })
                .copied();

            let drm_mode = match drm_mode {
                Some(m) => m,
                None => {
                    warn!("Connector {:?} has no modes, skipping", conn_handle);
                    continue;
                }
            };

            // Find a free CRTC via encoder compatibility mask.
            let crtc = conn
                .encoders()
                .iter()
                .filter_map(|&enc| drm.get_encoder(enc).ok())
                .flat_map(|enc| resources.filter_crtcs(enc.possible_crtcs()))
                .find(|c| !used_crtcs.contains(c));

            let crtc = match crtc {
                Some(c) => c,
                None => {
                    warn!("No free CRTC for connector {:?}, skipping", conn_handle);
                    continue;
                }
            };
            used_crtcs.insert(crtc);

            // ── Smithay Output ───────────────────────────────────────────────
            let (w, h) = drm_mode.size();
            let sm_mode = SmithayMode {
                size: (w as i32, h as i32).into(),
                refresh: drm_mode.vrefresh() as i32 * 1000,
            };
            let output = Output::new(
                format!("{:?}-{}", conn.interface(), conn.interface_id()),
                PhysicalProperties {
                    size: (0, 0).into(),
                    subpixel: Subpixel::Unknown,
                    make: "Pancake".into(),
                    model: drm_mode.name().to_string_lossy().into_owned(),
                },
            );
            output.change_current_state(
                Some(sm_mode),
                Some(Transform::Normal),
                None,
                Some((0, 0).into()),
            );
            output.set_preferred(sm_mode);

            // ── DRM surface ──────────────────────────────────────────────────
            let surface = match drm.create_surface(crtc, drm_mode, &[conn_handle]) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to create DRM surface for CRTC {crtc:?}: {e}");
                    continue;
                }
            };

            // ── Allocator + framebuffer exporter ─────────────────────────────
            let allocator = GbmAllocator::new(
                gbm.clone(),
                GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
            );
            let exporter = GbmFramebufferExporter::new(gbm.clone(), Some(drm_node));

            // ── DrmCompositor ─────────────────────────────────────────────────
            let renderer_formats: Vec<DrmFormat> =
                renderer.dmabuf_formats().iter().copied().collect();
            let mut compositor = match DrmCompositor::new(
                &output,
                surface,
                None,
                allocator,
                exporter,
                [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888],
                renderer_formats,
                drm.cursor_size(),
                Some(gbm.clone()),
            ) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to create DrmCompositor: {e}");
                    continue;
                }
            };

            compositor.reset_state()?;
            info!(
                "Output {:?}-{} mode={}x{}@{}Hz crtc={:?}",
                conn.interface(),
                conn.interface_id(),
                w,
                h,
                drm_mode.vrefresh(),
                crtc
            );
            outputs.push(OutputState {
                output,
                compositor,
                size: (w, h),
                frame_seq: 0,
            });
        }

        if outputs.is_empty() {
            warn!("GPU {:?}: no active outputs found", path);
        }

        // Import cursor image as a texture buffer for render-element cursor rendering
        let cursor_img = cursor::load_default();
        let cursor_hotspot = (cursor_img.hot_x, cursor_img.hot_y);
        let cursor_buffer: Option<TextureBuffer<GlesTexture>> =
            renderer.import_memory(
                &cursor_img.pixels,
                smithay::backend::allocator::Fourcc::Abgr8888,
                (cursor_img.width as i32, cursor_img.height as i32).into(),
                false,
            )
            .ok()
            .map(|tex| TextureBuffer::from_texture(&renderer, tex, 1, Transform::Normal, None));
        if cursor_buffer.is_none() {
            warn!("GPU {:?}: cursor texture import failed; no cursor will be rendered", path);
        }

        Ok((
            GpuData {
                drm,
                gbm,
                renderer,
                outputs,
                node: drm_node,
                cursor_buffer,
                cursor_hotspot,
            },
            notifier,
        ))
    }

    /// Render one frame on every output.
    pub fn render_all(&mut self, state: &PancakeState) {
        let clear = Color32F::new(0.07, 0.10, 0.20, 1.0);

        // Build cursor element once — reused for each output
        let cursor_elem: Option<TextureRenderElement<GlesTexture>> =
            self.cursor_buffer.as_ref().map(|buf| {
                // Subtract hotspot so the visual tip lands at the pointer position
                let hot = self.cursor_hotspot;
                let x = state.cursor_pos.x - hot.0 as f64;
                let y = state.cursor_pos.y - hot.1 as f64;
                let phys = smithay::utils::Point::<f64, smithay::utils::Physical>::from((x, y));
                TextureRenderElement::from_texture_buffer(
                    phys,
                    buf,
                    None,
                    None,
                    None,
                    Kind::Cursor,
                )
            });

        for out_state in &mut self.outputs {
            if let Err(e) = out_state.compositor.reset_state() {
                warn!("failed to force full DRM repaint: {e}");
            }

            // Collect space elements (windows, popups)
            let space_elements: Vec<smithay::desktop::space::SpaceRenderElements<
                GlesRenderer,
                smithay::wayland::compositor::WaylandSurfaceRenderElement<GlesRenderer>,
            >> = match state.space.render_elements_for_output(
                &mut self.renderer,
                &out_state.output,
                1.0,
            ) {
                Ok(e) => e,
                Err(err) => {
                    warn!("failed to collect render elements: {err}");
                    Vec::new()
                }
            };

            // Wrap in PancakeElements and append cursor on top
            let mut all: Vec<PancakeElements> = space_elements
                .into_iter()
                .map(PancakeElements::Space)
                .collect();
            if let Some(ref ce) = cursor_elem {
                all.push(PancakeElements::Cursor(ce.clone()));
            }

            let frame_flags = if std::env::var_os("PANCAKE_DRM_ALLOW_SCANOUT").is_some() {
                FrameFlags::DEFAULT
            } else {
                FrameFlags::empty()
            };

            match out_state.compositor.render_frame(
                &mut self.renderer,
                &all,
                clear,
                frame_flags,
            ) {
                Ok(result) if !result.is_empty => {
                    out_state.frame_seq = out_state.frame_seq.saturating_add(1);
                    if out_state.frame_seq <= 3 {
                        info!(
                            "Queued DRM frame {} for {}x{} output ({} elements, cursor={})",
                            out_state.frame_seq,
                            out_state.size.0,
                            out_state.size.1,
                            all.len(),
                            cursor_elem.is_some(),
                        );
                    }
                    if let Err(e) = out_state.compositor.queue_frame(()) {
                        error!("queue_frame failed: {e}");
                    }
                }
                Ok(_) => {}
                Err(e) => error!("render_frame failed: {e}"),
            }
        }
    }

    pub fn reset_outputs(&mut self) {
        for out_state in &mut self.outputs {
            if let Err(e) = out_state.compositor.reset_state() {
                warn!("failed to reset DRM output state: {e}");
            }
        }
    }

    /// Call on `DrmEvent::VBlank` to advance the swap chain and allow next frame.
    pub fn on_vblank(&mut self, _crtc: crtc::Handle) {
        for out_state in &mut self.outputs {
            let _ = out_state.compositor.frame_submitted();
        }
    }
}
