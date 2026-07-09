use smithay::{
    desktop::{PopupKind, Window},
    reexports::wayland_server::protocol::wl_seat::WlSeat,
    utils::{Serial, SERIAL_COUNTER},
    wayland::{
        seat::WaylandFocus,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

use crate::shell::layout;
use crate::state::PancakeState;

impl XdgShellHandler for PancakeState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // ── Toplevels ─────────────────────────────────────────────────────────────

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let geometry = layout::initial_geometry(&self.space);
        tracing::info!(
            "New XDG toplevel; mapping at {},{} size {}x{}",
            geometry.loc.x, geometry.loc.y, geometry.size.w, geometry.size.h
        );
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
            state.size = None;
        });
        surface.send_configure();

        let focus_surface = surface.wl_surface().clone();
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window.clone(), geometry.loc, true);
        self.space.raise_element(&window, true);

        // Register in the active workspace (pass current focused so BSP splits it)
        let prev_focused = self.focused_window.clone();
        self.workspaces.add_window(window.clone(), geometry.loc, prev_focused.as_ref());

        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(focus_surface), SERIAL_COUNTER.next_serial());
        }
        self.focused_window = Some(window);

        // Retile if workspace is in tiling mode
        self.retile();

        tracing::info!(
            "Workspace {}: {} windows",
            self.workspaces.active + 1,
            self.workspaces.active_windows().len()
        );
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let target = surface.wl_surface().clone();
        let window = self
            .space
            .elements()
            .find(|w| w.wl_surface().as_deref() == Some(&target))
            .cloned();
        if let Some(w) = window {
            if self.focused_window.as_ref() == Some(&w) {
                self.focused_window = None;
            }
            // Cancel any in-progress move or resize grab for this window
            if self.move_grab.as_ref().map(|(mw, _)| mw == &w).unwrap_or(false) {
                self.move_grab = None;
            }
            if self.resize_grab.as_ref().map(|(rw, ..)| rw == &w).unwrap_or(false) {
                self.resize_grab = None;
            }
            self.workspaces.remove_window(&w);
            self.space.unmap_elem(&w);
            // Retile to fill the vacated slot
            self.retile();
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: WlSeat, _serial: Serial) {
        // Start an interactive move grab: pointer offset = pointer - window top-left
        let win = self.space.elements()
            .find(|w| w.wl_surface().as_deref() == Some(surface.wl_surface()))
            .cloned();
        if let Some(win) = win {
            let win_loc = self.space.element_geometry(&win)
                .map(|g| g.loc)
                .unwrap_or_default();
            let offset = smithay::utils::Point::<f64, smithay::utils::Logical>::from((
                self.cursor_pos.x - win_loc.x as f64,
                self.cursor_pos.y - win_loc.y as f64,
            ));
            self.move_grab = Some((win, offset));
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: WlSeat,
        _serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let win = self.space.elements()
            .find(|w| w.wl_surface().as_deref() == Some(surface.wl_surface()))
            .cloned();
        if let Some(win) = win {
            let start_size = self.space.element_geometry(&win)
                .map(|g| g.size)
                .unwrap_or_default();
            self.resize_grab = Some((win, edges, self.cursor_pos, start_size));
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(output) = self.space.outputs().next() {
            let geo = self.space.output_geometry(output).unwrap_or_default();
            surface.with_pending_state(|s| {
                s.states.set(xdg_toplevel::State::Maximized);
                s.size = Some(geo.size);
            });
        }
        surface.send_pending_configure();
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Maximized);
            s.size = None;
        });
        surface.send_pending_configure();
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        surface.with_pending_state(|s| {
            s.states.set(xdg_toplevel::State::Fullscreen);
        });
        surface.send_pending_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Fullscreen);
            s.size = None;
        });
        surface.send_pending_configure();
    }

    // ── Popups ────────────────────────────────────────────────────────────────

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        if let Err(e) = self.popup_manager.track_popup(PopupKind::Xdg(surface)) {
            tracing::warn!("Failed to track popup: {e}");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|s| {
            s.geometry = positioner.get_geometry();
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {}
}
