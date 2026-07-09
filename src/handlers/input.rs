use smithay::{
    backend::input::{
        Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
        Seat, SeatHandler, SeatState,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Size, SERIAL_COUNTER},
    wayland::seat::WaylandFocus,
};
use wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;

use crate::shell::NavDir;
use crate::state::{PancakeState, SnapDirection};

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
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        match event {
            InputEvent::Keyboard { event } => self.handle_keyboard::<B>(event),
            InputEvent::PointerMotion { event } => self.handle_pointer_motion::<B>(event),
            InputEvent::PointerMotionAbsolute { event } => self.handle_pointer_motion_abs::<B>(event),
            InputEvent::PointerButton { event } => self.handle_pointer_button::<B>(event),
            InputEvent::PointerAxis { event } => self.handle_pointer_axis::<B>(event),
            _ => {}
        }
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────

    fn handle_keyboard<B: InputBackend>(&mut self, event: B::KeyboardKeyEvent) {
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
                    if !press {
                        return FilterResult::Forward;
                    }

                    // ── Super+1..9 — switch workspace ────────────────────────
                    let digit = match keysym.modified_sym() {
                        Keysym::_1 => Some(0), Keysym::_2 => Some(1), Keysym::_3 => Some(2),
                        Keysym::_4 => Some(3), Keysym::_5 => Some(4), Keysym::_6 => Some(5),
                        Keysym::_7 => Some(6), Keysym::_8 => Some(7), Keysym::_9 => Some(8),
                        _ => None,
                    };
                    if let Some(idx) = digit {
                        if mods.logo && mods.shift {
                            // Super+Shift+N — move focused window to workspace N
                            if let Some(win) = state.focused_window.clone() {
                                state.workspaces.move_window_to(&mut state.space, &win, idx);
                                state.focused_window = None;
                            }
                            return FilterResult::Intercept(());
                        } else if mods.logo {
                            // Super+N — switch to workspace N
                            if state.workspaces.switch_to(&mut state.space, idx) {
                                // Focus the top window on the new workspace
                                state.focused_window = None;
                                if let Some(win) = state.space.elements().last().cloned() {
                                    use smithay::wayland::seat::WaylandFocus;
                                    if let Some(surf) = win.wl_surface() {
                                        if let Some(kb) = state.seat.get_keyboard() {
                                            kb.set_focus(state, Some(surf.into_owned()), serial);
                                        }
                                    }
                                    state.focused_window = Some(win);
                                }
                            }
                            return FilterResult::Intercept(());
                        }
                    }

                    // ── Super+Q — close focused window ───────────────────────
                    if mods.logo && keysym.modified_sym() == Keysym::q {
                        if let Some(surf) = keyboard.current_focus() {
                            if let Some(w) = state.space.elements()
                                .find(|w| w.wl_surface().as_deref() == Some(&surf))
                                .cloned()
                            {
                                if let Some(t) = w.toplevel() { t.send_close(); }
                            }
                        }
                        return FilterResult::Intercept(());
                    }

                    // ── Super+T / Super+Return — launch configured terminal ──
                    if mods.logo && (keysym.modified_sym() == Keysym::t
                        || keysym.modified_sym() == Keysym::Return) {
                        let term = state.config.terminal.clone();
                        tracing::info!("Spawning terminal: {term}");
                        let _ = std::process::Command::new(&term).spawn()
                            .map_err(|e| tracing::warn!("Failed to spawn {term}: {e}"));
                        return FilterResult::Intercept(());
                    }

                    // ── Super+D — app launcher (wofi / rofi / bemenu) ────────
                    if mods.logo && keysym.modified_sym() == Keysym::d {
                        spawn_launcher();
                        return FilterResult::Intercept(());
                    }

                    // ── Super+Space — toggle tiling / floating ───────────────
                    if mods.logo && keysym.modified_sym() == Keysym::space {
                        state.toggle_tiling();
                        return FilterResult::Intercept(());
                    }

                    // ── Super+Tab — cycle focus ──────────────────────────────
                    if mods.logo && keysym.modified_sym() == Keysym::Tab {
                        state.cycle_focus(serial);
                        return FilterResult::Intercept(());
                    }

                    // ── Super+F — fullscreen toggle ──────────────────────────
                    if mods.logo && keysym.modified_sym() == Keysym::f {
                        if let Some(win) = state.focused_window.clone() {
                            if let Some(tl) = win.toplevel() {
                                let is_fs = tl.current_state().states
                                    .contains(wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen);
                                tl.with_pending_state(|s| {
                                    if is_fs {
                                        s.states.unset(wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen);
                                        s.size = None;
                                    } else {
                                        s.states.set(wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen);
                                        if let Some(geo) = state.space.outputs().next()
                                            .and_then(|o| state.space.output_geometry(o))
                                        {
                                            s.size = Some(geo.size);
                                        }
                                    }
                                });
                                tl.send_pending_configure();
                            }
                        }
                        return FilterResult::Intercept(());
                    }

                    // ── Super+arrows — tiling focus nav or floating snap ─────
                    if mods.logo && !mods.shift && !mods.ctrl {
                        let nav = match keysym.modified_sym() {
                            Keysym::Left  => Some(NavDir::Left),
                            Keysym::Right => Some(NavDir::Right),
                            Keysym::Up    => Some(NavDir::Up),
                            Keysym::Down  => Some(NavDir::Down),
                            _ => None,
                        };
                        if let Some(d) = nav {
                            if state.workspaces.is_tiling() {
                                state.focus_tile(d, serial);
                            } else {
                                let sd = match d {
                                    NavDir::Left  => SnapDirection::Left,
                                    NavDir::Right => SnapDirection::Right,
                                    NavDir::Up    => SnapDirection::Up,
                                    NavDir::Down  => SnapDirection::Down,
                                };
                                state.snap_focused(sd);
                            }
                            return FilterResult::Intercept(());
                        }
                    }

                    // ── Super+Shift+arrows — swap tiles (tiling only) ────────
                    if mods.logo && mods.shift && !mods.ctrl {
                        let nav = match keysym.modified_sym() {
                            Keysym::Left  => Some(NavDir::Left),
                            Keysym::Right => Some(NavDir::Right),
                            Keysym::Up    => Some(NavDir::Up),
                            Keysym::Down  => Some(NavDir::Down),
                            _ => None,
                        };
                        if let Some(d) = nav {
                            if state.workspaces.is_tiling() {
                                state.swap_tile(d);
                                return FilterResult::Intercept(());
                            }
                        }
                    }

                    // ── Super+Ctrl+arrows — resize tile ──────────────────────
                    if mods.logo && mods.ctrl && !mods.shift {
                        let nav = match keysym.modified_sym() {
                            Keysym::Left  => Some(NavDir::Left),
                            Keysym::Right => Some(NavDir::Right),
                            Keysym::Up    => Some(NavDir::Up),
                            Keysym::Down  => Some(NavDir::Down),
                            _ => None,
                        };
                        if let Some(d) = nav {
                            if state.workspaces.is_tiling() {
                                state.resize_tile(d);
                                return FilterResult::Intercept(());
                            }
                        }
                    }

                    // ── Super+Escape / Alt+Escape — quit compositor ──────────
                    if (mods.logo || mods.alt) && keysym.modified_sym() == Keysym::Escape {
                        tracing::info!("Escape chord — exiting Pancake");
                        std::process::exit(0);
                    }

                    // ── Super+L — lock screen ────────────────────────────────
                    if mods.logo && keysym.modified_sym() == Keysym::l {
                        spawn_bg("swaylock", &["-f", "-c", "0d1a33"]);
                        return FilterResult::Intercept(());
                    }

                    // ── Print / Super+Shift+S — screenshot ───────────────────
                    if keysym.modified_sym() == Keysym::Print {
                        spawn_screenshot(false);
                        return FilterResult::Intercept(());
                    }
                    if mods.logo && mods.shift && keysym.modified_sym() == Keysym::s {
                        spawn_screenshot(true); // area select
                        return FilterResult::Intercept(());
                    }

                    // ── Audio keys ───────────────────────────────────────────
                    if keysym.modified_sym() == Keysym::XF86_AudioMute {
                        spawn_bg("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "toggle"]);
                        return FilterResult::Intercept(());
                    }
                    if keysym.modified_sym() == Keysym::XF86_AudioRaiseVolume {
                        spawn_bg("pactl", &["set-sink-volume", "@DEFAULT_SINK@", "+5%"]);
                        return FilterResult::Intercept(());
                    }
                    if keysym.modified_sym() == Keysym::XF86_AudioLowerVolume {
                        spawn_bg("pactl", &["set-sink-volume", "@DEFAULT_SINK@", "-5%"]);
                        return FilterResult::Intercept(());
                    }

                    // ── Brightness keys ──────────────────────────────────────
                    if keysym.modified_sym() == Keysym::XF86_MonBrightnessUp {
                        spawn_bg("brightnessctl", &["set", "10%+"]);
                        return FilterResult::Intercept(());
                    }
                    if keysym.modified_sym() == Keysym::XF86_MonBrightnessDown {
                        spawn_bg("brightnessctl", &["set", "10%-"]);
                        return FilterResult::Intercept(());
                    }

                    FilterResult::Forward
                },
            );
        }
    }

    // ── Pointer motion ────────────────────────────────────────────────────────

    fn handle_pointer_motion<B: InputBackend>(&mut self, event: B::PointerMotionEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(pointer) = self.seat.get_pointer() {
            let current = pointer.current_location();
            let delta: Point<f64, Logical> = (event.delta_x(), event.delta_y()).into();
            let new_pos = Self::clamp_cursor_to_output(&self.space, current + delta);
            self.cursor_pos = new_pos;

            // Drag the grabbed window
            if let Some((ref win, ref offset)) = self.move_grab.clone() {
                let win = win.clone();
                let new_loc: Point<i32, Logical> = (
                    (new_pos.x - offset.x) as i32,
                    (new_pos.y - offset.y) as i32,
                ).into();
                self.space.map_element(win.clone(), new_loc, true);
                self.workspaces.update_position(&win, new_loc);
            }

            // Resize the grabbed window
            self.apply_resize_grab(new_pos);

            let focus = self.surface_under(new_pos);
            pointer.motion(
                self,
                focus,
                &MotionEvent { location: new_pos, serial, time: event.time_msec() },
            );
            pointer.frame(self);
        }
    }

    // ── Pointer motion (absolute — tablet, VM virtio-tablet, touchscreen) ────

    fn handle_pointer_motion_abs<B: InputBackend>(&mut self, event: B::PointerMotionAbsoluteEvent) {
        use smithay::backend::input::AbsolutePositionEvent;
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(pointer) = self.seat.get_pointer() {
            // Map the 0..1 absolute position onto the first output's geometry
            let new_pos = if let Some(geo) = self.space.outputs()
                .next()
                .and_then(|o| self.space.output_geometry(o))
            {
                let x = event.x_transformed(geo.size.w) + geo.loc.x as f64;
                let y = event.y_transformed(geo.size.h) + geo.loc.y as f64;
                Self::clamp_cursor_to_output(&self.space, (x, y).into())
            } else {
                (event.x(), event.y()).into()
            };

            self.cursor_pos = new_pos;

            if let Some((ref win, ref offset)) = self.move_grab.clone() {
                let win = win.clone();
                let new_loc: Point<i32, Logical> = (
                    (new_pos.x - offset.x) as i32,
                    (new_pos.y - offset.y) as i32,
                ).into();
                self.space.map_element(win.clone(), new_loc, true);
                self.workspaces.update_position(&win, new_loc);
            }

            self.apply_resize_grab(new_pos);

            let focus = self.surface_under(new_pos);
            pointer.motion(self, focus, &MotionEvent {
                location: new_pos,
                serial,
                time: event.time_msec(),
            });
            pointer.frame(self);
        }
    }

    // ── Pointer button ────────────────────────────────────────────────────────

    fn handle_pointer_button<B: InputBackend>(&mut self, event: B::PointerButtonEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(pointer) = self.seat.get_pointer() {
            let location = pointer.current_location();

            if event.state() == ButtonState::Pressed {
                // Collect window under pointer first
                let window_under: Option<smithay::desktop::Window> = {
                    let focus = self.surface_under(location);
                    focus.as_ref().and_then(|(surf, _)| {
                        self.space
                            .elements()
                            .find(|w| w.wl_surface().as_deref() == Some(surf))
                            .cloned()
                    })
                };

                let logo_held = self.seat.get_keyboard()
                    .map(|kb| kb.modifier_state().logo)
                    .unwrap_or(false);

                // LMB: check decoration hit test FIRST
                if event.button_code() == 0x110u32 {
                    use crate::render::decorations::{hit_test, DecoHit};
                    if let Some(hit) = hit_test(&self.space, location) {
                        let hit_win = match &hit {
                            DecoHit::Close(w) | DecoHit::Minimize(w)
                            | DecoHit::Maximize(w) | DecoHit::TitleBar(w) => w.clone(),
                        };
                        match hit {
                            DecoHit::Close(win) => {
                                if let Some(t) = win.toplevel() { t.send_close(); }
                            }
                            DecoHit::Maximize(win) => {
                                if let Some(geo) = self.output_geo() {
                                    if let Some(t) = win.toplevel() {
                                        t.with_pending_state(|s| {
                                            s.states.set(wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized);
                                            s.size = Some(geo.size);
                                        });
                                        t.send_pending_configure();
                                        self.space.map_element(win.clone(), geo.loc, true);
                                    }
                                }
                            }
                            DecoHit::Minimize(_win) => {
                                // TODO: minimise to taskbar
                            }
                            DecoHit::TitleBar(win) => {
                                let win_loc = self.space.element_geometry(&win)
                                    .map(|g| g.loc)
                                    .unwrap_or_default();
                                let offset = Point::<f64, Logical>::from((
                                    location.x - win_loc.x as f64,
                                    location.y - win_loc.y as f64,
                                ));
                                self.move_grab = Some((win.clone(), offset));
                                self.space.raise_element(&win, true);
                                self.focused_window = Some(win.clone());
                            }
                        }
                        // Focus the hit window
                        use smithay::wayland::seat::WaylandFocus;
                        if let Some(surf) = hit_win.wl_surface() {
                            if let Some(kb) = self.seat.get_keyboard() {
                                kb.set_focus(self, Some(surf.into_owned()), serial);
                            }
                        }
                        // Unconditional assignment: get_or_insert would leave
                        // focused_window stale (pointing at whatever window
                        // was focused before) when clicking a decoration
                        // button on a window that wasn't already focused,
                        // even though keyboard focus was just moved above.
                        self.focused_window = Some(hit_win);
                        pointer.button(self, &ButtonEvent {
                            button: event.button_code(), state: event.state(), serial, time: event.time_msec(),
                        });
                        pointer.frame(self);
                        return;
                    }
                }

                // Super+LMB → interactive move (0x110 = BTN_LEFT)
                if event.button_code() == 0x110u32 && logo_held {
                    if let Some(ref win) = window_under {
                        let win_loc = self.space.element_geometry(win)
                            .map(|g| g.loc)
                            .unwrap_or_default();
                        let offset = Point::<f64, Logical>::from((
                            location.x - win_loc.x as f64,
                            location.y - win_loc.y as f64,
                        ));
                        self.move_grab = Some((win.clone(), offset));
                    }
                }

                // Super+RMB → interactive resize from nearest corner (0x111 = BTN_RIGHT)
                if event.button_code() == 0x111u32 && logo_held {
                    if let Some(ref win) = window_under {
                        let geo = self.space.element_geometry(win).unwrap_or_default();
                        let cx = geo.loc.x as f64 + geo.size.w as f64 / 2.0;
                        let cy = geo.loc.y as f64 + geo.size.h as f64 / 2.0;
                        let edge = match (location.x >= cx, location.y >= cy) {
                            (true,  true)  => ResizeEdge::BottomRight,
                            (false, true)  => ResizeEdge::BottomLeft,
                            (true,  false) => ResizeEdge::TopRight,
                            (false, false) => ResizeEdge::TopLeft,
                        };
                        self.resize_grab = Some((
                            win.clone(),
                            edge,
                            location,
                            geo.size,
                        ));
                    }
                }

                // Click-to-focus
                if let Some((surf, _)) = self.surface_under(location) {
                    let win = self.space.elements()
                        .find(|w| w.wl_surface().as_deref() == Some(&surf))
                        .cloned();
                    if let Some(keyboard) = self.seat.get_keyboard() {
                        keyboard.set_focus(self, Some(surf), serial);
                    }
                    if win.is_some() {
                        self.focused_window = win;
                    }
                }

                // Raise clicked window
                if let Some(win) = window_under {
                    self.space.raise_element(&win, true);
                }
            } else if event.state() == ButtonState::Released {
                // End any active move or resize grab on button release
                self.move_grab = None;
                self.resize_grab = None;
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

    // ── Pointer axis ──────────────────────────────────────────────────────────

    fn handle_pointer_axis<B: InputBackend>(&mut self, event: B::PointerAxisEvent) {
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

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn clamp_cursor_to_output(
        space: &smithay::desktop::Space<smithay::desktop::Window>,
        pos: Point<f64, Logical>,
    ) -> Point<f64, Logical> {
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

// ── Resize grab application ───────────────────────────────────────────────────

impl PancakeState {
    pub fn apply_resize_grab(&mut self, new_pos: Point<f64, Logical>) {
        let Some((win, edge, start_pos, start_size)) = self.resize_grab.clone() else {
            return;
        };

        let dx = (new_pos.x - start_pos.x) as i32;
        let dy = (new_pos.y - start_pos.y) as i32;
        const MIN_W: i32 = 120;
        const MIN_H: i32 = 80;

        let new_w = match edge {
            ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight =>
                (start_size.w + dx).max(MIN_W),
            ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft =>
                (start_size.w - dx).max(MIN_W),
            _ => start_size.w,
        };
        let new_h = match edge {
            ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight =>
                (start_size.h + dy).max(MIN_H),
            ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight =>
                (start_size.h - dy).max(MIN_H),
            _ => start_size.h,
        };

        if let Some(toplevel) = win.toplevel() {
            toplevel.with_pending_state(|s| {
                s.size = Some(Size::from((new_w, new_h)));
            });
            toplevel.send_pending_configure();
        }
    }
}

// ── App launcher ─────────────────────────────────────────────────────────────

fn spawn_launcher() {
    let candidates = [
        ("wofi",   &["--show", "drun"] as &[&str]),
        ("rofi",   &["-show", "drun"]),
        ("bemenu-run", &[]),
        ("dmenu_run",  &[]),
    ];
    for (bin, args) in &candidates {
        if std::process::Command::new(bin).args(*args).spawn().is_ok() {
            tracing::info!("Launched app launcher: {bin}");
            return;
        }
    }
    tracing::warn!("No app launcher found (tried wofi, rofi, bemenu-run, dmenu_run)");
}

fn spawn_bg(cmd: &str, args: &[&str]) {
    let _ = std::process::Command::new(cmd).args(args).spawn()
        .map_err(|e| tracing::warn!("Failed to spawn {cmd}: {e}"));
}

fn spawn_screenshot(area: bool) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let path = format!("{home}/Pictures/screenshot-{timestamp}.png");
    let _ = std::fs::create_dir_all(format!("{home}/Pictures"));

    if area {
        // grim + slurp for area selection
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("grim -g \"$(slurp)\" \"{path}\""))
            .spawn();
    } else {
        spawn_bg("grim", &[&path]);
    }
    tracing::info!("Screenshot → {path}");
}
