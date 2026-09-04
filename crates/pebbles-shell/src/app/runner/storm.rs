//! Dev-only input monkey-tester: `PEBBLES_INPUT_STORM=1` drives synthetic hovers,
//! wheels, taps, double-taps, drags and key presses through the SAME dispatch
//! paths real winit input takes — against the real, running app. Combine with the
//! gallery's `GALLERY_TOUR` hook for a full-app burn-in that exercises every
//! screen under interaction, not just under display. Deterministic (fixed-seed
//! xorshift), so a crash it finds is a crash it finds every run.

#[allow(clippy::wildcard_imports)]
use super::*;

/// The storm's deterministic RNG + pacing state.
pub(super) struct InputStorm {
    rng: u64,
    /// Total dispatched steps (progress logging every ~1000).
    steps: u64,
    /// Optional clamp region (logical px, `PEBBLES_STORM_REGION="x0,y0,x1,y1"`)
    /// — every synthetic point lands inside it. Use it to pin the storm to ONE
    /// screen's content area (e.g. exclude the nav sidebar so the storm can
    /// never navigate away from the screen under test).
    region: Option<(f64, f64, f64, f64)>,
}

impl InputStorm {
    /// Build from the environment: any non-empty, non-"0" `PEBBLES_INPUT_STORM`.
    pub(super) fn from_env() -> Option<Self> {
        let v = std::env::var("PEBBLES_INPUT_STORM").ok()?;
        if v.is_empty() || v == "0" {
            return None;
        }
        let region = std::env::var("PEBBLES_STORM_REGION").ok().and_then(|s| {
            let v: Vec<f64> = s.split(',').filter_map(|n| n.trim().parse().ok()).collect();
            (v.len() == 4).then(|| (v[0], v[1], v[2], v[3]))
        });
        eprintln!("pebbles: INPUT STORM armed — synthetic input will hammer this window");
        if let Some(r) = region {
            eprintln!("pebbles: storm pinned to region {r:?}");
        }
        Some(InputStorm { rng: 0x9E37_79B9_7F4A_7C15, steps: 0, region })
    }

    /// xorshift64 — cheap, deterministic, good enough for a monkey.
    fn next(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
}

impl Runner {
    /// One storm turn: a handful of synthetic input steps against the main window,
    /// mirroring the dispatch sequences of the real `CursorMoved` / `MouseWheel` /
    /// `MouseInput` / `KeyboardInput` arms (see `window_event`).
    pub(super) fn storm_tick(&mut self) {
        // Move the storm out so `self` stays free for dispatch calls.
        let Some(mut storm) = self.storm.take() else { return };
        let Some((w, h)) = self.active.as_ref().map(|a| {
            let scale = a.window.scale_factor();
            let phys = a.window.inner_size();
            (phys.width as f64 / scale, phys.height as f64 / scale)
        }) else {
            self.storm = Some(storm);
            return;
        };
        if w < 2.0 || h < 2.0 {
            self.storm = Some(storm);
            return;
        }
        self.ui.make_current();

        // Occasionally resize the REAL window (compositor round-trip, surface
        // reconfigure) — maximize-like jumps a user makes that dispatch-level
        // input never exercises. RARE on purpose: each Wayland resize stalls the
        // loop on a configure round-trip, so frequent ones throttle the storm.
        if storm.next() % 1500 == 0
            && let Some(a) = self.active.as_ref()
        {
            let nw = 700 + (storm.next() % 900) as u32;
            let nh = 500 + (storm.next() % 500) as u32;
            let _ = a.window.request_inner_size(LogicalSize::new(nw, nh));
        }

        // The point pool: the whole window, or the pinned region intersected
        // with it.
        let (rx0, ry0, rx1, ry1) = match storm.region {
            Some((a, b, c, d)) => (a.min(w), b.min(h), c.min(w), d.min(h)),
            None => (0.0, 0.0, w, h),
        };
        let (rw, rh) = ((rx1 - rx0).max(1.0), (ry1 - ry0).max(1.0));
        for _ in 0..6 {
            storm.steps += 1;
            let p =
                Offset::new(rx0 + (storm.next() % rw as u64) as f64, ry0 + (storm.next() % rh as u64) as f64);
            match storm.next() % 12 {
                // Hover sweep — the highest-volume real-world input.
                0..=3 => {
                    self.cursor = p;
                    let _ = self.ui.dispatch_hover(p);
                    let _ = self.ui.cursor_at(p);
                }
                // Wheel — up to 3 lines either way, through the overlay router.
                4..=5 => {
                    let lines = (1 + storm.next() % 3) as f64;
                    let dy = if storm.next() & 1 == 0 { lines } else { -lines } * LINE_SCROLL;
                    let _ = wheel_with_overlay(&mut self.ui, p, dy);
                }
                // Click — through the REAL pointer pipeline (dispatch_pointer), the
                // exact path a winit MouseInput takes, so the storm tests it too.
                6..=7 => {
                    self.cursor = p;
                    let _ = self.dispatch_pointer(p, MouseButton::Left, ElementState::Pressed);
                    let _ = self.dispatch_pointer(p, MouseButton::Left, ElementState::Released);
                }
                // Drag: press, a few moves, release — pans, text drag-select, sliders.
                8 => {
                    self.cursor = p;
                    pebbles_core::focus::set_focus(None);
                    let armed = self.ui.tap_target_at(p);
                    let claimed = self.ui.begin_content_drag(p);
                    let pan = if claimed { None } else { self.ui.pan_target_at(p) };
                    if let Some(t) = pan {
                        let _ = self.ui.dispatch_pan_start(t, p);
                    }
                    let _ = self.ui.dispatch_pointer_down(p);
                    for _ in 0..3 {
                        let q = Offset::new(
                            (p.x + (storm.next() % 240) as f64 - 120.0).clamp(rx0, rx1),
                            (p.y + (storm.next() % 160) as f64 - 80.0).clamp(ry0, ry1),
                        );
                        self.cursor = q;
                        if claimed {
                            let _ = self.ui.update_content_drag(q);
                        } else if let Some(t) = pan {
                            let _ = self.ui.dispatch_pan_update(t, q);
                        }
                        let _ = self.ui.dispatch_hover(q);
                    }
                    let end = self.cursor;
                    let _ = self.ui.end_content_drag(end);
                    if let Some(t) = pan {
                        self.ui.dispatch_pan_end(t, end);
                    }
                    let _ = self.ui.dispatch_pointer_up(end);
                    if let Some(a) = armed {
                        let _ = self.ui.dispatch_tap_cancel(a);
                    }
                }
                // Right-click: the full secondary-tap sequence, falling through
                // to the global menu when nothing claims it (mirrors the shell).
                9 => {
                    self.cursor = p;
                    let down = self.ui.dispatch_secondary_tap_down(p);
                    let up = self.ui.dispatch_secondary_tap_up(p);
                    let tap = self.ui.dispatch_secondary_tap(p);
                    if !down && !up && !tap {
                        pebbles_widgets::global_menu::show(p.x, p.y);
                    }
                }
                // Long-press: down → begin → moves → end on a long-press target.
                10 => {
                    self.cursor = p;
                    if let Some(t) = self.ui.long_press_target_at(p) {
                        self.ui.dispatch_long_press_down(t, p);
                        let _ = self.ui.dispatch_long_press_begin(t, p);
                        let q = Offset::new((p.x + 24.0).min(rx1), (p.y + 12.0).min(ry1));
                        let _ = self.ui.dispatch_long_press_move(t, q);
                        self.ui.dispatch_long_press_end(t, q);
                    }
                }
                // Keys: editing commands, shortcut tokens, Tab focus, activation —
                // the same precedence chain as the real KeyboardInput arm.
                _ => {
                    use pebbles_core::{KeyInput as KI, Motion as M, ShortcutKey as SK};
                    let roll = storm.next() % 12;
                    let cmd: Option<KI> = match roll {
                        0 => Some(KI::Move { motion: M::Left, extend: false }),
                        1 => Some(KI::Move { motion: M::Right, extend: true }),
                        2 => Some(KI::Move { motion: M::WordLeft, extend: true }),
                        3 => Some(KI::Move { motion: M::LineEnd, extend: false }),
                        4 => Some(KI::Backspace),
                        5 => Some(KI::Delete),
                        6 => Some(KI::Insert("é—x".into())),
                        7 => Some(KI::SelectAll),
                        8 => Some(KI::Escape),
                        _ => None,
                    };
                    let handled = cmd.is_some_and(|ki| self.ui.dispatch_key(ki));
                    if !handled {
                        let mods =
                            pebbles_core::Mods { shift: roll == 9, ctrl: false, alt: false, meta: false };
                        let sk = match roll {
                            9 => SK::ArrowDown,
                            10 => SK::ArrowUp,
                            _ => SK::Escape,
                        };
                        if !pebbles_core::shortcuts::dispatch(self.ui.window_id(), mods, sk) {
                            let _ = self.ui.focus_move(true);
                        }
                    }
                }
            }
        }
        if storm.steps.is_multiple_of(1200) {
            eprintln!("pebbles: storm at {} steps — still alive", storm.steps);
        }
        self.storm = Some(storm);
        self.request_redraw();
    }
}
