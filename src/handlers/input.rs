use smithay::{
    backend::input::{
        Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
        PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
        Seat, SeatHandler, SeatState,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
    wayland::seat::WaylandFocus,
};

use crate::state::PancakeState;

// ── SeatHandler ─────────────────────────────────────────────────────────────

impl SeatHandler for PancakeState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}
}

// ── Raw input dispatch ───────────────────────────────────────────────────────

impl PancakeState {
    /// Route every raw input event from the active backend into seat state.
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        match event {
            InputEvent::Keyboard { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = event.time_msec();
                let press = event.state() == KeyState::Pressed;

                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        time,
                        |state, mods, keysym| {
                            use smithay::input::keyboard::Keysym;

                            // Super+Q → ask focused window to close
                            if press && mods.logo && keysym.modified_sym() == Keysym::q {
                                if let Some(surf) = keyboard.current_focus() {
                                    let win = state
                                        .space
                                        .elements()
                                        .find(|w| w.wl_surface().as_deref() == Some(&surf))
                                        .cloned();
                                    if let Some(w) = win {
                                        if let Some(t) = w.toplevel() {
                                            t.send_close();
                                        }
                                    }
                                }
                            }

                            // Super+T → launch terminal
                            if press && mods.logo && keysym.modified_sym() == Keysym::t {
                                let term = state.config.terminal.clone();
                                tracing::info!("Spawning terminal: {term}");
                                if let Err(e) = std::process::Command::new(&term).spawn() {
                                    tracing::warn!("Failed to spawn {term}: {e}");
                                }
                            }

                            // Super+Tab → cycle window focus
                            if press && mods.logo && keysym.modified_sym() == Keysym::Tab {
                                state.cycle_focus(serial);
                            }

                            // Super+Escape → quit
                            if press && mods.logo && keysym.modified_sym() == Keysym::Escape {
                                tracing::info!("Super+Escape — exiting Pancake");
                                std::process::exit(0);
                            }

                            FilterResult::Forward
                        },
                    );
                }
            }

            InputEvent::PointerMotion { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                if let Some(pointer) = self.seat.get_pointer() {
                    let current = pointer.current_location();
                    let delta: Point<f64, Logical> = (event.delta_x(), event.delta_y()).into();
                    let new_pos = current + delta;
                    // Clamp to output bounds so the cursor can't escape the screen
                    let new_pos = Self::clamp_cursor_to_output(&self.space, new_pos);
                    self.cursor_pos = new_pos;
                    let focus = self.surface_under(new_pos);
                    pointer.motion(
                        self,
                        focus,
                        &MotionEvent {
                            location: new_pos,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    pointer.frame(self);
                }
            }

            InputEvent::PointerButton { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                if let Some(pointer) = self.seat.get_pointer() {
                    let location = pointer.current_location();

                    if event.state() == ButtonState::Pressed {
                        // Collect window before any mutations to avoid borrow conflict.
                        let raise_target: Option<smithay::desktop::Window> = {
                            let focus = self.surface_under(location);
                            focus.as_ref().and_then(|(surf, _)| {
                                self.space
                                    .elements()
                                    .find(|w| w.wl_surface().as_deref() == Some(surf))
                                    .cloned()
                            })
                        };

                        // Click-to-focus + track focused window
                        if let Some((surf, _)) = self.surface_under(location) {
                            let win = self
                                .space
                                .elements()
                                .find(|w| w.wl_surface().as_deref() == Some(&surf))
                                .cloned();
                            if let Some(keyboard) = self.seat.get_keyboard() {
                                keyboard.set_focus(self, Some(surf), serial);
                            }
                            if win.is_some() {
                                self.focused_window = win;
                            }
                        }

                        // Raise
                        if let Some(win) = raise_target {
                            self.space.raise_element(&win, true);
                        }
                    }

                    pointer.button(
                        self,
                        &ButtonEvent {
                            button: event.button_code(),
                            state: event.state(),
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    pointer.frame(self);
                }
            }

            InputEvent::PointerAxis { event } => {
                if let Some(pointer) = self.seat.get_pointer() {
                    let mut frame = AxisFrame::new(event.time_msec()).source(AxisSource::Wheel);
                    if let Some(v) = event.amount(Axis::Vertical) {
                        frame = frame.value(Axis::Vertical, v);
                    }
                    if let Some(v) = event.amount(Axis::Horizontal) {
                        frame = frame.value(Axis::Horizontal, v);
                    }
                    pointer.axis(self, frame);
                    pointer.frame(self);
                }
            }

            _ => {}
        }
    }

    /// Clamp `pos` to the union of all mapped output rects.
    #[allow(dead_code)]
    fn clamp_cursor_to_output(space: &smithay::desktop::Space<smithay::desktop::Window>, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        let mut x0 = 0.0f64;
        let mut y0 = 0.0f64;
        let mut x1 = 1.0f64;
        let mut y1 = 1.0f64;
        for output in space.outputs() {
            if let Some(geo) = space.output_geometry(output) {
                x0 = x0.min(geo.loc.x as f64);
                y0 = y0.min(geo.loc.y as f64);
                x1 = x1.max((geo.loc.x + geo.size.w) as f64);
                y1 = y1.max((geo.loc.y + geo.size.h) as f64);
            }
        }
        Point::from((pos.x.clamp(x0, x1 - 1.0), pos.y.clamp(y0, y1 - 1.0)))
    }

    /// Topmost Wayland surface under `point` and its local pixel offset.
    pub fn surface_under(
        &self,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(point)
            .and_then(|(window, win_loc)| {
                let local: Point<f64, Logical> = point - win_loc.to_f64();
                window
                    .surface_under(local, smithay::desktop::WindowSurfaceType::ALL)
                    .map(|(s, off)| (s, off.to_f64()))
            })
    }
}
