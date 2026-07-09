use std::os::unix::io::OwnedFd;

use smithay::{
    delegate_compositor, delegate_data_device, delegate_layer_shell, delegate_output, delegate_seat,
    delegate_shm, delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    reexports::wayland_server::{
        backend::{ClientData, ClientId, DisconnectReason},
        protocol::wl_buffer,
        Display, DisplayHandle,
    },
    utils::{Logical, Point},
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorState},
        output::{OutputHandler, OutputManagerState},
        selection::{
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        shell::{wlr_layer::WlrLayerShellState, xdg::{decoration::XdgDecorationState, XdgShellState}},
        shm::{ShmHandler, ShmState},
    },
    xwayland::xwm::X11Wm,
};

use crate::config::{Config, RELOAD_REQUESTED};
use crate::render::AeroRenderer;
use crate::shell::{workspace::WorkspaceManager, NavDir};

// ── Per-client state ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _id: ClientId) {}
    fn disconnected(&self, _id: ClientId, _reason: DisconnectReason) {}
}

// ── Compositor-wide state ───────────────────────────────────────────────────

#[allow(dead_code)]
pub struct PancakeState {
    pub display_handle: DisplayHandle,

    // Core Wayland protocols
    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,

    // XDG shell (application windows + decorations)
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub popup_manager: PopupManager,

    // Input
    pub seat_state: SeatState<Self>,
    pub seat: Seat<Self>,

    // Clipboard + drag-and-drop
    pub data_device_state: DataDeviceState,

    // Layout space — windows live here
    pub space: Space<Window>,

    // Virtual workspace manager
    pub workspaces: WorkspaceManager,

    // XWayland window manager (started lazily)
    pub xwm: Option<X11Wm>,

    // Aero glass rendering pipeline
    pub renderer: AeroRenderer,

    // Runtime configuration (terminal command, etc.)
    pub config: Config,

    // Currently focused window, tracked for Super+Tab cycling and borders.
    pub focused_window: Option<Window>,

    // Current pointer position (logical coordinates). Updated in PointerMotion
    // handler and read by the render backends to draw the cursor sprite.
    pub cursor_pos: Point<f64, Logical>,

    // Interactive move grab: (window being moved, pointer-to-window-top-left offset).
    pub move_grab: Option<(Window, Point<f64, Logical>)>,

    // Interactive resize grab: (window, edge, start cursor pos, start window size).
    pub resize_grab: Option<(Window, wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge, Point<f64, Logical>, smithay::utils::Size<i32, Logical>)>,
}

impl PancakeState {
    pub fn new(display: &Display<Self>) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&dh, "pancake");
        seat.add_keyboard(Default::default(), 200, 25)
            .expect("keyboard init failed");
        seat.add_pointer();

        let config = Config::load();
        let mut renderer = AeroRenderer::default();
        renderer.apply_config(&config);

        Self {
            display_handle: dh,
            compositor_state,
            shm_state,
            output_manager_state,
            xdg_shell_state,
            xdg_decoration_state,
            layer_shell_state,
            popup_manager: PopupManager::default(),
            seat_state,
            seat,
            data_device_state,
            space: Space::default(),
            workspaces: WorkspaceManager::new(),
            xwm: None,
            renderer,
            config,
            focused_window: None,
            cursor_pos: (0.0, 0.0).into(),
            move_grab: None,
            resize_grab: None,
        }
    }

    /// Reload config from disk and apply any changed values.
    pub fn reload_config(&mut self) {
        use std::sync::atomic::Ordering;
        RELOAD_REQUESTED.store(false, Ordering::Relaxed);
        self.config = Config::load();
        self.renderer.apply_config(&self.config);
        tracing::info!("Config reloaded");
    }

    /// Get the primary output geometry.
    pub fn output_geo(&self) -> Option<smithay::utils::Rectangle<i32, smithay::utils::Logical>> {
        self.space.outputs().next().and_then(|o| self.space.output_geometry(o))
    }

    /// Re-apply the BSP tiling layout for the active workspace.
    /// No-op when the workspace is in floating mode.
    pub fn retile(&mut self) {
        if let Some(geo) = self.output_geo() {
            self.workspaces.apply_tiles(&mut self.space, geo);
        }
    }

    /// Toggle tiling on the active workspace and re-layout.
    pub fn toggle_tiling(&mut self) {
        if let Some(geo) = self.output_geo() {
            self.workspaces.toggle_tiling(&mut self.space, geo);
        }
    }

    /// Move keyboard focus to the neighboring tile in `dir` (tiling mode only).
    pub fn focus_tile(&mut self, dir: NavDir, serial: smithay::utils::Serial) {
        let focused = match self.focused_window.clone() { Some(w) => w, None => return };
        let geo = match self.output_geo() { Some(g) => g, None => return };
        let neighbor = self.workspaces.tile_neighbor(&focused, dir, geo);
        if let Some(win) = neighbor {
            self.space.raise_element(&win, true);
            use smithay::wayland::seat::WaylandFocus;
            if let Some(surf) = win.wl_surface() {
                if let Some(kb) = self.seat.get_keyboard() {
                    kb.set_focus(self, Some(surf.into_owned()), serial);
                }
            }
            self.focused_window = Some(win);
        }
    }

    /// Swap focused tile with its neighbor in `dir` and re-layout.
    pub fn swap_tile(&mut self, dir: NavDir) {
        let focused = match self.focused_window.clone() { Some(w) => w, None => return };
        let geo = match self.output_geo() { Some(g) => g, None => return };
        if self.workspaces.swap_neighbor(&focused, dir, geo) {
            self.workspaces.apply_tiles(&mut self.space, geo);
        }
    }

    /// Resize focused tile by moving the split ratio.
    pub fn resize_tile(&mut self, dir: NavDir) {
        const STEP: f32 = 0.05;
        let focused = match self.focused_window.clone() { Some(w) => w, None => return };
        let geo = match self.output_geo() { Some(g) => g, None => return };
        let delta = match dir {
            NavDir::Right | NavDir::Down => STEP,
            NavDir::Left  | NavDir::Up   => -STEP,
        };
        self.workspaces.adjust_ratio(&focused, delta);
        self.workspaces.apply_tiles(&mut self.space, geo);
    }

    /// Check if a SIGHUP-triggered reload is pending and, if so, reload.
    pub fn maybe_reload_config(&mut self) {
        use std::sync::atomic::Ordering;
        if RELOAD_REQUESTED.load(Ordering::Relaxed) {
            self.reload_config();
        }
    }

    /// Cycle keyboard focus to the next window in the space.
    pub fn cycle_focus(&mut self, serial: smithay::utils::Serial) {
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        if windows.is_empty() {
            return;
        }

        let next = if let Some(cur) = &self.focused_window {
            let pos = windows.iter().position(|w| w == cur);
            match pos {
                Some(i) => windows[(i + 1) % windows.len()].clone(),
                None => windows[0].clone(),
            }
        } else {
            windows[0].clone()
        };

        self.space.raise_element(&next, true);

        use smithay::wayland::seat::WaylandFocus;
        if let Some(surf) = next.wl_surface() {
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, Some(surf.into_owned()), serial);
            }
        }

        self.focused_window = Some(next);
    }

    /// Snap the focused window to a half of the output (left / right) or
    /// maximize / restore (up / down).  Inspired by Hyprland's Super+arrow layout.
    pub fn snap_focused(
        &mut self,
        direction: SnapDirection,
    ) {
        let output_geo = match self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        {
            Some(g) => g,
            None => return,
        };

        let win = match self.focused_window.clone() {
            Some(w) => w,
            None => return,
        };

        let (loc, size) = match direction {
            SnapDirection::Left => (
                output_geo.loc,
                (output_geo.size.w / 2, output_geo.size.h).into(),
            ),
            SnapDirection::Right => (
                (output_geo.loc.x + output_geo.size.w / 2, output_geo.loc.y).into(),
                (output_geo.size.w / 2, output_geo.size.h).into(),
            ),
            SnapDirection::Up => (output_geo.loc, output_geo.size),
            SnapDirection::Down => {
                // Restore: cascade position, reasonable size
                let count = self.space.elements().count() as i32;
                let x = 64 + count * 32;
                let y = 64 + count * 32;
                (
                    (x, y).into(),
                    (output_geo.size.w * 2 / 3, output_geo.size.h * 2 / 3).into(),
                )
            }
        };

        if let Some(toplevel) = win.toplevel() {
            toplevel.with_pending_state(|s| {
                s.size = Some(size);
                if matches!(direction, SnapDirection::Up) {
                    s.states.set(wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized);
                } else {
                    s.states.unset(wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized);
                }
            });
            toplevel.send_pending_configure();
        }
        self.space.map_element(win.clone(), loc, true);
        self.workspaces.update_position(&win, loc);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SnapDirection {
    Left,
    Right,
    Up,
    Down,
}

// ── Required trait impls ─────────────────────────────────────────────────────

impl BufferHandler for PancakeState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for PancakeState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl OutputHandler for PancakeState {}

impl SelectionHandler for PancakeState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for PancakeState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for PancakeState {}

impl ServerDndGrabHandler for PancakeState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

// ── Protocol delegation macros ───────────────────────────────────────────────

delegate_compositor!(PancakeState);
delegate_xdg_shell!(PancakeState);
delegate_xdg_decoration!(PancakeState);
delegate_shm!(PancakeState);
delegate_output!(PancakeState);
delegate_layer_shell!(PancakeState);
delegate_seat!(PancakeState);
delegate_data_device!(PancakeState);
