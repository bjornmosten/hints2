use gtk::prelude::*;
use std::collections::HashMap;
use utils::{Child, Config};

pub struct OverlayWindow {
    window: gtk::Window,
    children: Vec<Child>,
    hints: HashMap<String, Child>,
    config: Config,
    pressed_keys: String,
    mouse_action: Option<MouseAction>,
}

#[derive(Debug, Clone)]
pub struct MouseAction {
    pub x: f64,
    pub y: f64,
    pub action: String,
    pub button: String,
    pub repeat: u32,
}

impl OverlayWindow {
    pub fn new(config: Config, children: Vec<Child>, hints: HashMap<String, Child>) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Popup);

        window.set_decorated(false);
        window.set_resizable(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.set_keep_above(true);
        window.set_accept_focus(false);
        window.set_focus_on_map(false);

        Self {
            window,
            children,
            hints,
            config,
            pressed_keys: String::new(),
            mouse_action: None,
        }
    }

    pub fn show(&self) {
        self.window.show();
    }

    pub fn hide(&self) {
        self.window.hide();
    }

    pub fn get_mouse_action(&self) -> Option<MouseAction> {
        self.mouse_action.clone()
    }

    fn draw_hint_label(&self, cr: &gdk::cairo::Context, label: &str, child: &Child) {
        let hints_config = &self.config.hints;

        let font_size = hints_config.hint_font_size as f64;

        cr.set_font_size(font_size);

        let extents = match cr.text_extents(label) {
            Ok(e) => e,
            Err(_) => return,
        };

        let width = extents.width() + hints_config.hint_width_padding as f64 * 2.0;
        let height = hints_config.hint_height as f64;

        let x = child.absolute_position.0;
        let y = child.absolute_position.1;

        cr.set_source_rgba(
            hints_config.hint_background_r,
            hints_config.hint_background_g,
            hints_config.hint_background_b,
            hints_config.hint_background_a,
        );
        cr.rectangle(x, y, width, height);
        cr.fill();

        cr.set_source_rgba(
            hints_config.hint_font_r,
            hints_config.hint_font_g,
            hints_config.hint_font_b,
            hints_config.hint_font_a,
        );

        let text_x = x + (width - extents.width()) / 2.0;
        let text_y = y + (height + extents.height()) / 2.0;

        cr.move_to(text_x, text_y);
        cr.show_text(label);
    }

    fn filter_hints(&mut self, key: char) {
        let alphabet = &self.config.alphabet;
        if !alphabet.contains(key) {
            return;
        }

        self.pressed_keys.push(key);

        let pressed = self.pressed_keys.clone();
        self.hints.retain(|label, _| label.starts_with(&pressed));

        if self.hints.len() == 1 {
            if let Some((_, child)) = self.hints.iter().next() {
                self.mouse_action = Some(MouseAction {
                    x: child.absolute_position.0,
                    y: child.absolute_position.1,
                    action: "click".to_string(),
                    button: "left".to_string(),
                    repeat: 1,
                });
                self.hide();
            }
        }
    }
}
