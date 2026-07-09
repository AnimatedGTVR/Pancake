/// XWayland integration — allows legacy X11 applications to run inside Pancake.
use smithay::{
    desktop::Window,
    utils::{Logical, Rectangle},
    wayland::selection::SelectionTarget,
    xwayland::xwm::{Reorder, ResizeEdge, X11Wm, XwmHandler},
    xwayland::X11Surface,
};

use crate::shell::layout;
use crate::state::PancakeState;

impl XwmHandler for PancakeState {
    fn xwm_state(&mut self, _xwm: smithay::xwayland::xwm::XwmId) -> &mut X11Wm {
        self.xwm.as_mut().expect("XWM not initialised")
    }

    fn new_window(&mut self, _xwm: smithay::xwayland::xwm::XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _window: X11Surface,
    ) {
    }

    fn map_window_request(&mut self, _xwm: smithay::xwayland::xwm::XwmId, window: X11Surface) {
        window.set_mapped(true).ok();
        let w = Window::new_x11_window(window);
        let pos = layout::initial_geometry(&self.space).loc;
        self.space.map_element(w, pos, true);
    }

    fn mapped_override_redirect_window(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        window: X11Surface,
    ) {
        let w = Window::new_x11_window(window);
        self.space.map_element(w, (0, 0), false);
    }

    fn unmapped_window(&mut self, _xwm: smithay::xwayland::xwm::XwmId, window: X11Surface) {
        // Two-step: collect while space is immutably borrowed, then mutate.
        let found = self
            .space
            .elements()
            .find(|w| matches!(w.x11_surface(), Some(s) if s == &window))
            .cloned();
        if let Some(w) = found {
            self.space.unmap_elem(&w);
        }
        if !window.is_override_redirect() {
            window.set_mapped(false).ok();
        }
    }

    fn destroyed_window(&mut self, _xwm: smithay::xwayland::xwm::XwmId, window: X11Surface) {
        let found = self
            .space
            .elements()
            .find(|w| matches!(w.x11_surface(), Some(s) if s == &window))
            .cloned();
        if let Some(w) = found {
            self.space.unmap_elem(&w);
        }
    }

    fn configure_request(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        window: X11Surface,
        _x: Option<i32>,
        _y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let mut geo = window.geometry();
        if let Some(width) = w {
            geo.size.w = width as i32;
        }
        if let Some(height) = h {
            geo.size.h = height as i32;
        }
        let _ = window.configure(geo);
    }

    fn configure_notify(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
    }

    fn resize_request(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _window: X11Surface,
        _button: u32,
        _edges: ResizeEdge,
    ) {
    }

    fn move_request(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _window: X11Surface,
        _button: u32,
    ) {
    }

    fn allow_selection_access(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _sel: SelectionTarget,
    ) -> bool {
        true
    }

    fn send_selection(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _sel: SelectionTarget,
        _mime_type: String,
        _fd: std::os::unix::io::OwnedFd,
    ) {
    }

    fn new_selection(
        &mut self,
        _xwm: smithay::xwayland::xwm::XwmId,
        _sel: SelectionTarget,
        _mime_types: Vec<String>,
    ) {
    }

    fn cleared_selection(&mut self, _xwm: smithay::xwayland::xwm::XwmId, _sel: SelectionTarget) {}
}
