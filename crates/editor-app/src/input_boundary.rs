//! Window-owned input ownership across document exchange and synchronous native panels.
//! Backend events have no native timestamp: an observed release rearms a key only
//! in the next input batch. Neither backend `repeat` nor elapsed time proves ownership.
use eframe::egui;
use std::collections::HashSet;

const BUTTONS: [egui::PointerButton; 5] = [
    egui::PointerButton::Primary,
    egui::PointerButton::Secondary,
    egui::PointerButton::Middle,
    egui::PointerButton::Extra1,
    egui::PointerButton::Extra2,
];

pub(crate) struct Boundary {
    ctx: Option<egui::Context>,
    pub(crate) serial: u64,
    native_serial: u64,
    raw_batch: u64,
    blocked_frame: Option<u64>,
    drain_return: bool,
    held: HashSet<egui::Key>,
    quarantine: HashSet<egui::Key>,
    released: HashSet<egui::Key>,
    fresh_releases: HashSet<egui::Key>,
    pub(crate) held_buttons: [bool; 5],
    text_armed: bool,
    composition: bool,
    fresh_buttons: [bool; 5],
}
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Diagnostics {
    pub boundary_serial: u64,
    pub received_held_keys: u16,
    pub logical_held_keys: u16,
    pub shortcut_held_keys: u16,
    pub quarantined_keys: u16,
    pub control_keys: u8,
    pub modifiers: u8,
    pub held_buttons: u8,
    pub drain_return: bool,
    pub text_armed: bool,
}
impl Default for Boundary {
    fn default() -> Self {
        Self {
            ctx: None,
            serial: 0,
            native_serial: 0,
            raw_batch: 0,
            blocked_frame: None,
            drain_return: false,
            held: HashSet::new(),
            quarantine: HashSet::new(),
            released: HashSet::new(),
            fresh_releases: HashSet::new(),
            held_buttons: [false; 5],
            text_armed: true,
            composition: true,
            fresh_buttons: [false; 5],
        }
    }
}
impl Boundary {
    pub(crate) fn bind(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
    }
    pub(crate) fn discard_owner(&mut self) {
        self.text_armed = false;
        self.composition = false;
        self.fresh_buttons.fill(false);
    }
    pub(crate) fn return_pending(&self) -> bool {
        self.drain_return
    }
    pub(crate) fn native_serial(&self) -> u64 {
        self.native_serial
    }
    pub(crate) fn raw_batch(&self) -> u64 {
        self.raw_batch
    }
    pub(crate) fn context(&self) -> Option<&egui::Context> {
        self.ctx.as_ref()
    }
    pub(crate) fn blocked(&self) -> bool {
        self.ctx
            .as_ref()
            .is_some_and(|ctx| self.blocked_frame == Some(ctx.cumulative_frame_nr()))
    }
    pub(crate) fn quarantined(&self) -> &HashSet<egui::Key> {
        &self.quarantine
    }
    pub(crate) fn diagnostics(&self, shortcuts: usize) -> Diagnostics {
        let (logical, modifiers) = self.ctx.as_ref().map_or((0, egui::Modifiers::NONE), |ctx| {
            ctx.input(|i| (i.keys_down.len(), i.modifiers))
        });
        Diagnostics {
            boundary_serial: self.serial,
            received_held_keys: self.held.len() as u16,
            logical_held_keys: logical as u16,
            shortcut_held_keys: shortcuts as u16,
            quarantined_keys: self.quarantine.len() as u16,
            // Fixed control-key bits only; no text, paths, coordinates or full key stream.
            control_keys: [egui::Key::O, egui::Key::Enter, egui::Key::Escape]
                .iter()
                .enumerate()
                .fold(0, |bits, (n, key)| {
                    bits | (u8::from(self.held.contains(key)) << n)
                        | (u8::from(self.quarantine.contains(key)) << (n + 3))
                }),
            modifiers: u8::from(modifiers.shift)
                | (u8::from(modifiers.ctrl) << 1)
                | (u8::from(modifiers.alt) << 2)
                | (u8::from(modifiers.mac_cmd) << 3)
                | (u8::from(modifiers.command) << 4),
            held_buttons: self
                .held_buttons
                .iter()
                .enumerate()
                .fold(0, |bits, (n, down)| bits | (u8::from(*down) << n)),
            drain_return: self.drain_return,
            text_armed: self.text_armed,
        }
    }
    pub(crate) fn begin(&mut self, native: bool) {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("input boundary exhausted");
        self.quarantine.extend(self.held.iter().copied());
        self.released.clear();
        self.fresh_releases.clear();
        self.text_armed = false;
        self.composition = false;
        self.fresh_buttons.fill(false);
        if native {
            self.native_serial = self.serial;
            // Panel activation/dismissal keys may arrive after runModal returns,
            // even when they were never delivered to the application before entry.
            self.quarantine
                .extend([egui::Key::Enter, egui::Key::Escape]);
            self.drain_return = true;
        }
        if let Some(ctx) = &self.ctx {
            self.blocked_frame = Some(ctx.cumulative_frame_nr());
            ctx.input(|i| {
                self.quarantine.extend(i.keys_down.iter().copied());
                for (index, button) in BUTTONS.iter().enumerate() {
                    self.held_buttons[index] |= i.pointer.button_down(*button);
                }
            });
            self.clear_context(ctx);
            ctx.request_repaint();
        }
    }
    fn clear_context(&self, ctx: &egui::Context) {
        ctx.input_mut(|i| {
            i.keys_down.retain(|key| !self.quarantine.contains(key));
            i.events.clear();
            i.raw.events.clear();
            i.raw.dropped_files.clear();
            i.pointer = Default::default();
            i.raw_scroll_delta = egui::Vec2::ZERO;
            i.smooth_scroll_delta = egui::Vec2::ZERO;
        });
    }
    pub(crate) fn native<T>(&mut self, choose: impl FnOnce() -> T) -> T {
        self.begin(true);
        let result = choose();
        // Covers OK, Cancel and every returned error. No lock crosses the panel.
        if let Some(ctx) = &self.ctx {
            self.blocked_frame = Some(ctx.cumulative_frame_nr());
            self.clear_context(ctx);
            ctx.request_repaint();
        }
        result
    }
    pub(crate) fn raw_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.raw_batch = self
            .raw_batch
            .checked_add(1)
            .expect("input batch exhausted");
        self.bind(ctx);
        for key in self.released.drain() {
            self.quarantine.remove(&key);
            self.fresh_releases.insert(key);
        }
        let drain = std::mem::take(&mut self.drain_return);
        if drain {
            self.blocked_frame = Some(ctx.cumulative_frame_nr());
            for event in &raw.events {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = event
                {
                    self.quarantine.insert(*key);
                }
            }
        }
        let poisoned = drain || raw.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key, .. } if self.quarantine.contains(key))
        });
        if poisoned {
            self.composition = false;
        }
        if !raw.focused && self.serial != 0 {
            self.text_armed = false;
            self.composition = false;
            self.fresh_buttons.fill(false);
        }
        let owner_allowed = !poisoned
            && raw.focused
            && !raw.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::WindowFocused(false) | egui::Event::PointerGone
                )
            });
        let previously_released = self.fresh_releases.clone();
        raw.events.retain(|event| {
            match event {
                egui::Event::Key { key, pressed, .. } => {
                    let old = self.quarantine.contains(key);
                    if *pressed {
                        let first = self.held.insert(*key);
                        self.released.remove(key); // release -> press in one batch stays isolated
                        self.text_armed |=
                            owner_allowed && !old && first && previously_released.contains(key);
                    } else {
                        self.held.remove(key);
                        if old {
                            self.released.insert(*key);
                        } else {
                            self.fresh_releases.insert(*key);
                        }
                    }
                    !drain && !old
                }
                egui::Event::PointerButton {
                    button, pressed, ..
                } => {
                    let index = BUTTONS.iter().position(|b| b == button).unwrap();
                    let previously_down = self.held_buttons[index];
                    self.held_buttons[index] = *pressed;
                    if *pressed && !previously_down && !drain && owner_allowed {
                        self.fresh_buttons[index] = true;
                    }
                    self.text_armed |= owner_allowed && self.fresh_buttons[index] && !*pressed;
                    if !*pressed {
                        self.fresh_buttons[index] = false;
                    }
                    !drain
                }
                egui::Event::PointerGone | egui::Event::WindowFocused(false) => {
                    self.held_buttons.fill(false);
                    self.held.clear();
                    self.composition = false;
                    self.fresh_buttons.fill(false);
                    if self.serial != 0 {
                        self.text_armed = false;
                    }
                    true // existing focus/cancel arbitration must still see this event
                }
                egui::Event::Text(_)
                | egui::Event::Paste(_)
                | egui::Event::Copy
                | egui::Event::Cut => self.text_armed && owner_allowed,
                egui::Event::Ime(egui::ImeEvent::Preedit(_)) => {
                    self.composition = self.text_armed && owner_allowed;
                    self.composition
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                    let allow =
                        (self.serial == 0 || self.composition) && self.text_armed && owner_allowed;
                    self.composition = false;
                    allow
                }
                egui::Event::Ime(egui::ImeEvent::Enabled) => self.text_armed && owner_allowed,
                egui::Event::Ime(egui::ImeEvent::Disabled) => {
                    self.composition = false;
                    true // Payload-free teardown must reach the App IME owner.
                }
                _ => !drain,
            }
        });
        if drain {
            raw.dropped_files.clear();
        }
        // The event-order owner gate never retroactively authorizes earlier
        // payloads in this batch. Native provenance of arbitrarily delayed
        // payloads remains unavailable from the backend.
        ctx.input_mut(|i| i.keys_down.retain(|key| !self.quarantine.contains(key)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn batch(b: &mut Boundary, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::RawInput {
        let mut raw = egui::RawInput {
            events,
            ..Default::default()
        };
        b.raw_input(ctx, &mut raw);
        let _ = ctx.run(raw.clone(), |_| {});
        raw
    }
    #[test]
    fn native_boundary_old_key_release_rearms_next_batch_without_extra_drain() {
        let ctx = egui::Context::default();
        let mut b = Boundary::default();
        batch(&mut b, &ctx, vec![key(egui::Key::O, true)]);
        b.native(|| ());
        assert!(batch(&mut b, &ctx, vec![]).events.is_empty());
        assert!(
            batch(&mut b, &ctx, vec![key(egui::Key::O, true)])
                .events
                .is_empty()
        );
        assert!(
            batch(
                &mut b,
                &ctx,
                vec![key(egui::Key::O, false), key(egui::Key::O, true)]
            )
            .events
            .is_empty()
        );
        assert!(
            batch(&mut b, &ctx, vec![key(egui::Key::O, true)])
                .events
                .is_empty()
        );
        assert!(
            batch(&mut b, &ctx, vec![key(egui::Key::O, false)])
                .events
                .is_empty()
        );
        assert_eq!(
            batch(
                &mut b,
                &ctx,
                vec![
                    key(egui::Key::O, true),
                    egui::Event::Text("fresh after release".into())
                ]
            )
            .events
            .len(),
            2
        );
    }
    #[test]
    fn native_boundary_delayed_enter_and_unowned_payloads_are_fenced() {
        let ctx = egui::Context::default();
        let mut b = Boundary::default();
        b.bind(&ctx);
        b.native(|| ());
        batch(&mut b, &ctx, vec![]);
        let payload = || {
            vec![
                egui::Event::Text("private test".into()),
                egui::Event::Paste("private test".into()),
                egui::Event::Ime(egui::ImeEvent::Commit("private test".into())),
            ]
        };
        assert!(batch(&mut b, &ctx, payload()).events.is_empty());
        batch(&mut b, &ctx, vec![]);
        let mut late = vec![key(egui::Key::Enter, true)];
        late.extend(payload());
        assert!(batch(&mut b, &ctx, late).events.is_empty());
        assert!(
            batch(&mut b, &ctx, vec![key(egui::Key::Enter, false)])
                .events
                .is_empty()
        );
        assert_eq!(
            batch(&mut b, &ctx, vec![key(egui::Key::Enter, true)])
                .events
                .len(),
            1
        );
    }
    #[test]
    fn native_boundary_mouse_rearms_text_without_retiring_old_o() {
        let ctx = egui::Context::default();
        let mut b = Boundary::default();
        batch(&mut b, &ctx, vec![key(egui::Key::O, true)]);
        b.native(|| ());
        batch(&mut b, &ctx, vec![]);
        for pressed in [true, false] {
            batch(
                &mut b,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: egui::pos2(20., 20.),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(b.quarantined().contains(&egui::Key::O));
        assert_eq!(
            batch(
                &mut b,
                &ctx,
                vec![
                    egui::Event::Text("fresh".into()),
                    egui::Event::Paste("fresh".into())
                ]
            )
            .events
            .len(),
            2
        );
        assert!(
            batch(
                &mut b,
                &ctx,
                vec![egui::Event::Ime(egui::ImeEvent::Commit("orphan".into()))]
            )
            .events
            .is_empty()
        );
        assert_eq!(
            batch(
                &mut b,
                &ctx,
                vec![
                    egui::Event::Ime(egui::ImeEvent::Preedit("fresh".into())),
                    egui::Event::Ime(egui::ImeEvent::Commit("fresh".into()))
                ]
            )
            .events
            .len(),
            2
        );
        assert!(
            batch(
                &mut b,
                &ctx,
                vec![key(egui::Key::O, true), egui::Event::Text("old".into())]
            )
            .events
            .is_empty()
        );
    }
    #[test]
    fn native_boundary_fresh_gesture_cannot_retroactively_authorize_old_payload() {
        let ctx = egui::Context::default();
        let mut b = Boundary::default();
        b.bind(&ctx);
        b.native(|| ());
        batch(&mut b, &ctx, vec![]);
        let button = |pressed| egui::Event::PointerButton {
            pos: egui::pos2(20., 20.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let result = batch(
            &mut b,
            &ctx,
            vec![
                egui::Event::Text("old".into()),
                egui::Event::Paste("old".into()),
                egui::Event::Ime(egui::ImeEvent::Preedit("old".into())),
                egui::Event::Ime(egui::ImeEvent::Commit("old".into())),
                button(true),
                button(false),
                egui::Event::Text("fresh".into()),
            ],
        );
        assert_eq!(
            result.events,
            vec![
                button(true),
                button(false),
                egui::Event::Text("fresh".into())
            ]
        );
    }
    #[test]
    fn native_boundary_pointer_or_focus_loss_invalidates_the_new_gesture() {
        for lost in [egui::Event::PointerGone, egui::Event::WindowFocused(false)] {
            let ctx = egui::Context::default();
            let mut b = Boundary::default();
            b.bind(&ctx);
            b.native(|| ());
            batch(&mut b, &ctx, vec![]);
            let button = |pressed| egui::Event::PointerButton {
                pos: egui::pos2(20., 20.),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            batch(&mut b, &ctx, vec![button(true)]);
            batch(&mut b, &ctx, vec![lost]);
            batch(&mut b, &ctx, vec![button(false)]);
            assert!(
                batch(&mut b, &ctx, vec![egui::Event::Text("old".into())])
                    .events
                    .is_empty()
            );
            let mut raw = egui::RawInput {
                focused: false,
                events: vec![button(true), button(false), egui::Event::Text("old".into())],
                ..Default::default()
            };
            b.raw_input(&ctx, &mut raw);
            assert!(!raw.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
        }
    }
    #[test]
    fn native_boundary_held_mouse_release_is_not_a_fresh_text_owner() {
        let ctx = egui::Context::default();
        let mut b = Boundary::default();
        let button = |pressed| egui::Event::PointerButton {
            pos: egui::pos2(20., 20.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        batch(&mut b, &ctx, vec![button(true)]);
        b.native(|| ());
        batch(&mut b, &ctx, vec![button(false)]);
        assert!(!b.held_buttons.iter().any(|down| *down));
        assert!(
            batch(&mut b, &ctx, vec![egui::Event::Text("old".into())])
                .events
                .is_empty()
        );
    }
}
