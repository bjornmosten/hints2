//! GTK3 overlay window that draws Cairo-painted hint labels over a target
//! application window. Ported from `hints/huds/overlay.py`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use cairo::{Context, FontSlant, FontWeight};
use gdk::prelude::*;
use gdk::{GrabStatus, SeatCapabilities};
use glib::Propagation;
use gtk::prelude::*;
use utils::{Child, Config};

/// Result describing the mouse interaction the user selected.
#[derive(Debug, Clone)]
pub struct MouseAction {
    pub x: f64,
    pub y: f64,
    pub action: String, // "click" | "hover" | "grab"
    pub button: String, // "left" | "right"
    pub repeat: u32,
}

impl Default for MouseAction {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            action: "click".to_string(),
            button: "left".to_string(),
            repeat: 1,
        }
    }
}

/// Shared mutable state between the owning `OverlayWindow` and the signal
/// closures attached to GTK widgets. GTK requires `'static` closures so we
/// lean on `Rc<RefCell<...>>` to share this state safely.
struct Inner {
    hints: HashMap<String, Child>,
    hint_selector_state: String,
    hints_drawn_offsets: HashMap<String, (f64, f64)>,
    config: Config,
    mouse_action: MouseAction,
    resolved: Option<MouseAction>,
    is_wayland: bool,
    // TODO: load `hint_x_offset`/`hint_y_offset` from config when those keys
    // are added to `utils::HintsConfig`.
    hint_shift_x: f64,
    hint_shift_y: f64,
}

pub struct OverlayWindow {
    window: gtk::Window,
    inner: Rc<RefCell<Inner>>,
}

impl OverlayWindow {
    /// Builds the overlay popup. The window is not shown yet; the caller is
    /// expected to invoke `gtk::main()` after `window().show_all()`.
    pub fn new(
        x_pos: i32,
        y_pos: i32,
        width: i32,
        height: i32,
        config: Config,
        hints: HashMap<String, Child>,
        is_wayland: bool,
    ) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Popup);

        // RGBA visual for transparent drawing.
        if let Some(screen) = GtkWindowExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }

        window.set_app_paintable(true);
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.set_keep_above(true);
        window.set_accept_focus(true);
        window.set_sensitive(true);
        window.set_default_size(width, height);
        window.move_(x_pos, y_pos);

        let drawing_area = gtk::DrawingArea::new();
        window.add(&drawing_area);

        let inner = Rc::new(RefCell::new(Inner {
            hints,
            hint_selector_state: String::new(),
            hints_drawn_offsets: HashMap::new(),
            config,
            mouse_action: MouseAction::default(),
            resolved: None,
            is_wayland,
            hint_shift_x: 0.0,
            hint_shift_y: 0.0,
        }));

        // Draw signal.
        {
            let inner = Rc::clone(&inner);
            drawing_area.connect_draw(move |_, cr| {
                draw_hints(cr, &inner);
                Propagation::Proceed
            });
        }

        // Key-press signal.
        {
            let inner = Rc::clone(&inner);
            let window_weak = window.downgrade();
            let area_weak = drawing_area.downgrade();
            window.connect_key_press_event(move |_, event| {
                let window = match window_weak.upgrade() {
                    Some(w) => w,
                    None => return Propagation::Proceed,
                };
                handle_key_press(&window, area_weak.upgrade().as_ref(), event, &inner);
                Propagation::Proceed
            });
        }

        // Destroy quits the main loop so main.rs can collect the result.
        window.connect_destroy(|_| gtk::main_quit());

        // Show signal: grab keyboard and hide cursor.
        {
            let inner = Rc::clone(&inner);
            window.connect_show(move |win| {
                on_show(win, &inner);
            });
        }

        Self { window, inner }
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }

    /// Returns the resolved mouse action after the GTK main loop has exited.
    pub fn take_mouse_action(&self) -> Option<MouseAction> {
        self.inner.borrow_mut().resolved.take()
    }
}

fn draw_hints(cr: &Context, inner: &Rc<RefCell<Inner>>) {
    let mut inner = inner.borrow_mut();
    let hints_config = inner.config.hints.clone();
    let hint_height = hints_config.hint_height as f64;
    let hint_shift_x = inner.hint_shift_x;
    let hint_shift_y = inner.hint_shift_y;
    let hint_upercase = hints_config.hint_upercase;
    let hint_selector_state = inner.hint_selector_state.clone();
    let hints = inner.hints.clone();

    cr.select_font_face(
        &hints_config.hint_font_face,
        FontSlant::Normal,
        FontWeight::Bold,
    );
    cr.set_font_size(hints_config.hint_font_size as f64);

    inner.hints_drawn_offsets.clear();

    for (hint_value, child) in hints.iter() {
        let (x_loc, y_loc) = child.relative_position;
        if x_loc < 0.0 || y_loc < 0.0 {
            continue;
        }

        let utf8 = if hint_upercase {
            hint_value.to_uppercase()
        } else {
            hint_value.clone()
        };
        let hint_state = if hint_upercase {
            hint_selector_state.to_uppercase()
        } else {
            hint_selector_state.clone()
        };

        let extents = match cr.text_extents(&utf8) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let x_bearing = extents.x_bearing();
        let y_bearing = extents.y_bearing();
        let text_width = extents.width();
        let text_height = extents.height();
        let hint_width = text_width + hints_config.hint_width_padding as f64;

        let hint_x_offset = child.width / 2.0 - hint_width / 2.0 + hint_shift_x;
        let hint_y_offset = child.height / 2.0 - hint_height / 2.0 + hint_shift_y;

        let hint_x = x_loc + hint_x_offset;
        let hint_y = y_loc + hint_y_offset;

        let _ = cr.save();
        cr.new_path();
        cr.translate(hint_x, hint_y);

        inner.hints_drawn_offsets.insert(
            hint_value.clone(),
            (
                hint_x_offset + hint_width / 2.0,
                hint_y_offset + hint_height / 2.0,
            ),
        );

        cr.rectangle(0.0, 0.0, hint_width, hint_height);
        cr.set_source_rgba(
            hints_config.hint_background_r,
            hints_config.hint_background_g,
            hints_config.hint_background_b,
            hints_config.hint_background_a,
        );
        let _ = cr.fill();

        let text_x = (hint_width / 2.0) - (text_width / 2.0 + x_bearing);
        let text_y = (hint_height / 2.0) - (text_height / 2.0 + y_bearing);

        cr.move_to(text_x, text_y);
        cr.set_source_rgba(
            hints_config.hint_font_r,
            hints_config.hint_font_g,
            hints_config.hint_font_b,
            hints_config.hint_font_a,
        );
        let _ = cr.show_text(&utf8);

        cr.move_to(text_x, text_y);
        cr.set_source_rgba(
            hints_config.hint_pressed_font_r,
            hints_config.hint_pressed_font_g,
            hints_config.hint_pressed_font_b,
            hints_config.hint_pressed_font_a,
        );
        let _ = cr.show_text(&hint_state);

        cr.close_path();
        let _ = cr.restore();
    }
}

fn handle_key_press(
    window: &gtk::Window,
    drawing_area: Option<&gtk::DrawingArea>,
    event: &gdk::EventKey,
    inner: &Rc<RefCell<Inner>>,
) {
    let display = match gdk::Display::default() {
        Some(d) => d,
        None => return,
    };
    let keymap = match gdk::Keymap::for_display(&display) {
        Some(k) => k,
        None => return,
    };

    let keyval_event = event.keyval();
    let state = event.state();
    let masked_state = state - gdk::ModifierType::LOCK_MASK;

    let translated = keymap.translate_keyboard_state(
        event.hardware_keycode() as u32,
        masked_state,
        1,
    );
    let consumed = translated
        .map(|(_kv, _g, _l, consumed)| consumed)
        .unwrap_or(gdk::ModifierType::empty());

    let modifiers = state & gtk::accelerator_get_default_mod_mask() & !consumed;
    let keyval_lower = keyval_event.to_lower();

    let exit_key = inner.borrow().config.exit_key;
    if *keyval_lower == exit_key {
        gtk::main_quit();
        return;
    }

    {
        let mut st = inner.borrow_mut();
        if modifiers.bits() == st.config.hover_modifier {
            st.mouse_action.action = "hover".to_string();
        }
        if modifiers.bits() == st.config.grab_modifier {
            st.mouse_action.action = "grab".to_string();
        }
        if keyval_lower != keyval_event {
            // Shift (or other case-shifting modifier) held → right click.
            st.mouse_action.action = "click".to_string();
            st.mouse_action.button = "right".to_string();
        }
    }

    // Translate keyval → character.
    let hint_chr = match keyval_lower.to_unicode() {
        Some(c) => c,
        None => return,
    };

    if hint_chr.is_ascii_digit() {
        let mut st = inner.borrow_mut();
        let current = st.mouse_action.repeat;
        let as_str = if current == 0 {
            String::new()
        } else {
            current.to_string()
        };
        let combined = format!("{as_str}{hint_chr}");
        st.mouse_action.repeat = combined.parse().unwrap_or(current);
    }

    update_hints(hint_chr, inner);
    if let Some(area) = drawing_area {
        area.queue_draw();
    }

    // Terminal state: exactly one hint remaining.
    let resolved = {
        let st = inner.borrow();
        if st.hints.len() == 1 {
            let (label, child) = st.hints.iter().next().unwrap();
            let offset = st
                .hints_drawn_offsets
                .get(label)
                .copied()
                .unwrap_or((0.0, 0.0));
            let (abs_x, abs_y) = child.absolute_position;
            let mut ma = st.mouse_action.clone();
            ma.x = abs_x + offset.0;
            ma.y = abs_y + offset.1;
            if ma.repeat == 0 {
                ma.repeat = 1;
            }
            Some(ma)
        } else {
            None
        }
    };

    if let Some(ma) = resolved {
        if let Some(seat) = display.default_seat() {
            seat.ungrab();
        }
        inner.borrow_mut().resolved = Some(ma);
        window.close();
    }
}

fn update_hints(next_char: char, inner: &Rc<RefCell<Inner>>) {
    let mut st = inner.borrow_mut();
    let next_prefix = format!("{}{}", st.hint_selector_state, next_char);
    let filtered: HashMap<String, Child> = st
        .hints
        .iter()
        .filter(|(label, _)| label.starts_with(&next_prefix))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !filtered.is_empty() {
        st.hints = filtered;
        st.hint_selector_state.push(next_char);
    }
}

fn on_show(window: &gtk::Window, inner: &Rc<RefCell<Inner>>) {
    let is_wayland = inner.borrow().is_wayland;
    let display = match gdk::Display::default() {
        Some(d) => d,
        None => return,
    };
    let gdk_win = match window.window() {
        Some(w) => w,
        None => return,
    };

    if !is_wayland {
        if let Some(seat) = display.default_seat() {
            // Retry until the grab succeeds, mirroring the Python loop.
            loop {
                let status = seat.grab(
                    &gdk_win,
                    SeatCapabilities::KEYBOARD,
                    false,
                    None,
                    None,
                    None,
                );
                if status == GrabStatus::Success {
                    break;
                }
            }
        }
    }

    if let Some(cursor) = gdk::Cursor::from_name(&display, "none") {
        gdk_win.set_cursor(Some(&cursor));
    }
}
