//! Tiny 1x1 popup that steals keyboard focus so key presses intended for
//! mouse manipulation do not leak to the application underneath. Ported from
//! `hints/huds/interceptor.py`.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;

use gdk::prelude::*;
use gdk::SeatCapabilities;
use glib::Propagation;
use gtk::prelude::*;
use serde_json::{json, Map, Value};
use utils::{Config, UNIX_DOMAIN_SOCKET_FILE};

/// Minimal inline mouse IPC client. Kept local to this module so the file is
/// independently compilable from both the `hints` bin and lib targets without
/// depending on a sibling module that may not be visible in both trees.
struct MouseClient {
    socket_path: String,
}

impl MouseClient {
    fn new() -> Self {
        Self {
            socket_path: UNIX_DOMAIN_SOCKET_FILE.to_string(),
        }
    }

    fn send(&self, method: &str, args: Vec<Value>, kwargs: Map<String, Value>) -> Result<Value, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to mouse socket: {e}"))?;
        let message = json!({"method": method, "args": args, "kwargs": kwargs});
        stream
            .write_all(message.to_string().as_bytes())
            .map_err(|e| format!("Failed to send: {e}"))?;
        let mut buffer = [0u8; 4096];
        let n = stream
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read: {e}"))?;
        if n == 0 {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&buffer[..n]).map_err(|e| format!("Failed to parse: {e}"))
    }

    fn click(&self, x: f64, y: f64, button: &str, states: &[i64], repeat: u32, absolute: bool) -> Result<(), String> {
        let args = vec![json!(x), json!(y), json!(button), json!(states), json!(repeat)];
        let mut kwargs = Map::new();
        kwargs.insert("absolute".to_string(), json!(absolute));
        self.send("click", args, kwargs)?;
        Ok(())
    }

    fn move_(&self, x: f64, y: f64, absolute: bool) -> Result<(), String> {
        let args = vec![json!(x), json!(y)];
        let mut kwargs = Map::new();
        kwargs.insert("absolute".to_string(), json!(absolute));
        self.send("move", args, kwargs)?;
        Ok(())
    }

    fn do_mouse_action(&self, state: &Value, key: &str, mode: &str) -> Result<Value, String> {
        let args = vec![state.clone(), json!(key), json!(mode)];
        self.send("do_mouse_action", args, Map::new())
    }
}

struct Inner {
    action: String,
    config: Config,
    key_press_state: Value,
    is_wayland: bool,
    first_move: bool,
    client: MouseClient,
}

pub struct InterceptorWindow {
    window: gtk::Window,
    _inner: Rc<RefCell<Inner>>,
}

impl InterceptorWindow {
    pub fn new(
        x_pos: i32,
        y_pos: i32,
        width: i32,
        height: i32,
        action: String,
        config: Config,
        is_wayland: bool,
    ) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Popup);

        if let Some(screen) = GtkWindowExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }

        window.set_app_paintable(true);
        window.set_decorated(false);
        window.set_accept_focus(true);
        window.set_sensitive(true);
        window.set_default_size(width.max(1), height.max(1));
        window.move_(x_pos, y_pos);

        let inner = Rc::new(RefCell::new(Inner {
            action,
            config,
            key_press_state: json!({}),
            is_wayland,
            first_move: true,
            client: MouseClient::new(),
        }));

        window.connect_destroy(|_| gtk::main_quit());

        {
            let inner = Rc::clone(&inner);
            window.connect_key_release_event(move |_, _| {
                inner.borrow_mut().key_press_state = json!({});
                Propagation::Proceed
            });
        }

        {
            let inner = Rc::clone(&inner);
            window.connect_key_press_event(move |_, event| {
                handle_key_press(event, &inner);
                Propagation::Proceed
            });
        }

        {
            let inner = Rc::clone(&inner);
            window.connect_show(move |win| on_grab(win, &inner));
        }

        Self {
            window,
            _inner: inner,
        }
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }
}

fn handle_key_press(event: &gdk::EventKey, inner: &Rc<RefCell<Inner>>) {
    let display = match gdk::Display::default() {
        Some(d) => d,
        None => return,
    };
    let keymap = match gdk::Keymap::for_display(&display) {
        Some(k) => k,
        None => return,
    };

    let masked_state = event.state() - gdk::ModifierType::LOCK_MASK;
    let translated = keymap.translate_keyboard_state(
        event.hardware_keycode() as u32,
        masked_state,
        1,
    );
    let keyval = match translated {
        Some((kv, _g, _l, _consumed)) => gdk::keys::Key::from(kv),
        None => return,
    };
    let keyval_lower = keyval.to_lower();

    let exit_key = inner.borrow().config.exit_key;
    if *keyval_lower == exit_key {
        let client = MouseClient::new();
        let _ = client.click(0.0, 0.0, "left", &[0], 1, false);
        gtk::main_quit();
        return;
    }

    {
        let mut st = inner.borrow_mut();
        if st.first_move {
            // Hyprland workaround: nudge the mouse to force window focus.
            let _ = st.client.move_(0.0, 1.0, false);
            let _ = st.client.move_(0.0, -1.0, false);
            st.first_move = false;
        }
    }

    let Some(chr) = keyval_lower.to_unicode() else {
        return;
    };

    let mut st = inner.borrow_mut();
    let action = st.action.clone();
    let state_snapshot = st.key_press_state.clone();
    let key_str = chr.to_string();
    let result = match action.as_str() {
        "grab" => st
            .client
            .do_mouse_action(&state_snapshot, &key_str, "move"),
        "scroll" => st
            .client
            .do_mouse_action(&state_snapshot, &key_str, "scroll"),
        _ => return,
    };

    if let Ok(new_state) = result {
        st.key_press_state = new_state;
    }
}

fn on_grab(window: &gtk::Window, inner: &Rc<RefCell<Inner>>) {
    if inner.borrow().is_wayland {
        return;
    }
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let Some(gdk_win) = window.window() else {
        return;
    };
    if let Some(seat) = display.default_seat() {
        let _ = seat.grab(
            &gdk_win,
            SeatCapabilities::KEYBOARD,
            false,
            None,
            None,
            None,
        );
    }
}
