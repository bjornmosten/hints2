use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AbsInfo, AbsoluteAxisType, AttributeSet, EventType, InputEvent, Key, RelativeAxisType};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use utils::{MouseButton, MouseButtonState, MouseMode, UNIX_DOMAIN_SOCKET_FILE};

pub struct Mouse {
    screen_width: i32,
    screen_height: i32,
    write_pause: Duration,
    relative_device: VirtualDevice,
    absolute_device: VirtualDevice,
}

impl Mouse {
    pub fn new(screen_width: i32, screen_height: i32) -> Self {
        let mut rel_keys = AttributeSet::<Key>::new();
        rel_keys.insert(Key::BTN_LEFT);
        rel_keys.insert(Key::BTN_RIGHT);
        rel_keys.insert(Key::BTN_MIDDLE);

        let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
        rel_axes.insert(RelativeAxisType::REL_X);
        rel_axes.insert(RelativeAxisType::REL_Y);
        rel_axes.insert(RelativeAxisType::REL_HWHEEL);
        rel_axes.insert(RelativeAxisType::REL_WHEEL);

        let relative_device = VirtualDeviceBuilder::new()
            .unwrap()
            .name("Hints relative mouse")
            .with_keys(&rel_keys)
            .unwrap()
            .with_relative_axes(&rel_axes)
            .unwrap()
            .build()
            .unwrap();

        let mut abs_keys = AttributeSet::<Key>::new();
        abs_keys.insert(Key::BTN_LEFT);
        abs_keys.insert(Key::BTN_RIGHT);
        abs_keys.insert(Key::BTN_MIDDLE);

        let abs_x_info = AbsInfo::new(0, 0, screen_width, 0, 0, 0);
        let abs_y_info = AbsInfo::new(0, 0, screen_height, 0, 0, 0);

        let absolute_device = VirtualDeviceBuilder::new()
            .unwrap()
            .name("Hints absolute mouse")
            .with_keys(&abs_keys)
            .unwrap()
            .with_absolute_axis(&AbsoluteAxisType::ABS_X, &abs_x_info)
            .unwrap()
            .with_absolute_axis(&AbsoluteAxisType::ABS_Y, &abs_y_info)
            .unwrap()
            .build()
            .unwrap();

        Self {
            screen_width,
            screen_height,
            write_pause: Duration::from_millis(30),
            relative_device,
            absolute_device,
        }
    }

    pub fn scroll(&mut self, x: i32, y: i32) {
        let ev_h = InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_HWHEEL.0, x);
        let ev_v = InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_WHEEL.0, y);
        self.relative_device.emit(&[ev_h, ev_v]).unwrap();
        thread::sleep(self.write_pause);
    }

    pub fn move_mouse(&mut self, x: i32, y: i32, absolute: bool) {
        if absolute {
            let ev_x = InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_X.0, x);
            let ev_y = InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_Y.0, y);
            self.absolute_device.emit(&[ev_x, ev_y]).unwrap();
        } else {
            let ev_x = InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, x);
            let ev_y = InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, y);
            self.relative_device.emit(&[ev_x, ev_y]).unwrap();
        }
        thread::sleep(self.write_pause);
    }

    pub fn click(
        &mut self,
        x: i32,
        y: i32,
        button: MouseButton,
        button_states: &[MouseButtonState],
        repeat: u32,
        absolute: bool,
    ) {
        self.move_mouse(x, y, absolute);

        let key = match button {
            MouseButton::Left => Key::BTN_LEFT,
            MouseButton::Right => Key::BTN_RIGHT,
        };

        let device = if absolute {
            &mut self.absolute_device
        } else {
            &mut self.relative_device
        };

        for _ in 0..repeat {
            for state in button_states {
                let ev = InputEvent::new(EventType::KEY, key.code(), *state as i32);
                device.emit(&[ev]).unwrap();
                thread::sleep(self.write_pause);
            }
        }

        if absolute {
            // small move to clear previous write incase the previous move wants
            // to be repeated
            self.move_mouse(x + 1, y, true);
            self.move_mouse(x - 1, y, true);
        }
    }

    pub fn do_mouse_action(
        &mut self,
        key_press_state: &mut Option<(Instant, String, MouseMode)>,
        key: &str,
        mode: MouseMode,
    ) {
        let now = Instant::now();
        let rampup_time = Duration::from_secs_f64(0.5);
        let sensitivity = 10;

        let (start_time, _start_key, _start_mode) = match key_press_state.take() {
            Some((time, k, m)) => (time, k, m),
            None => {
                *key_press_state = Some((now, key.to_string(), mode));
                return;
            }
        };

        let elapsed = now.duration_since(start_time);
        let current_sensitivity = if elapsed > rampup_time {
            sensitivity as i32 * 2
        } else {
            sensitivity as i32
        };

        let delta = current_sensitivity;

        match (key, mode) {
            ("h", MouseMode::Move) | ("h", MouseMode::Scroll) => self.scroll(-delta, 0),
            ("l", MouseMode::Move) | ("l", MouseMode::Scroll) => self.scroll(delta, 0),
            ("k", MouseMode::Move) | ("k", MouseMode::Scroll) => self.scroll(0, -delta),
            ("j", MouseMode::Move) | ("j", MouseMode::Scroll) => self.scroll(0, delta),
            _ => {}
        }

        *key_press_state = Some((now, key.to_string(), mode));
    }

    pub fn set_screen_size(&mut self, width: i32, height: i32) {
        self.screen_width = width;
        self.screen_height = height;
    }
}

pub struct MouseService {
    mouse: Arc<Mutex<Mouse>>,
    listener: UnixListener,
}

impl MouseService {
    pub fn new(screen_width: i32, screen_height: i32) -> Self {
        if Path::new(UNIX_DOMAIN_SOCKET_FILE).exists() {
            fs::remove_file(UNIX_DOMAIN_SOCKET_FILE).ok();
        }

        let listener =
            UnixListener::bind(UNIX_DOMAIN_SOCKET_FILE).expect("Failed to bind to socket");

        let mouse = Mouse::new(screen_width, screen_height);

        Self {
            mouse: Arc::new(Mutex::new(mouse)),
            listener,
        }
    }

    pub fn run(&self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    self.handle_connection(stream);
                }
                Err(e) => {
                    eprintln!("Connection error: {}", e);
                }
            }
        }
    }

    fn handle_connection(&self, mut stream: UnixStream) {
        let mut buffer = [0u8; 1024];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let message = String::from_utf8_lossy(&buffer[..n]);
                    if let Some(response) = self.process_message(&message) {
                        stream.write_all(response.as_bytes()).ok();
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn process_message(&self, message: &str) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(message).ok()?;
        let method = json["method"].as_str()?;
        let args = &json["args"];
        let kwargs = &json["kwargs"];

        let mut mouse = self.mouse.lock().ok()?;

        match method {
            "click" => {
                let x = args.get(0)?.as_i64()? as i32;
                let y = args.get(1)?.as_i64()? as i32;
                let button = if args.get(2)?.as_str()? == "left" {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                };
                let button_states: Vec<MouseButtonState> = args
                    .get(3)?
                    .as_array()?
                    .iter()
                    .filter_map(|v| {
                        if v.as_i64()? == 1 {
                            Some(MouseButtonState::Down)
                        } else {
                            Some(MouseButtonState::Up)
                        }
                    })
                    .collect();
                let repeat = args.get(4)?.as_u64()? as u32;
                let absolute = kwargs["absolute"].as_bool().unwrap_or(false);

                mouse.click(x, y, button, &button_states, repeat, absolute);
            }
            "move" => {
                let x = args.get(0)?.as_i64()? as i32;
                let y = args.get(1)?.as_i64()? as i32;
                let absolute = kwargs["absolute"].as_bool().unwrap_or(false);
                mouse.move_mouse(x, y, absolute);
            }
            "scroll" => {
                let x = args.get(0)?.as_i64()? as i32;
                let y = args.get(1)?.as_i64()? as i32;
                mouse.scroll(x, y);
            }
            "do_mouse_action" => {
                let mut key_press_state: Option<(Instant, String, MouseMode)> = None;
                let key = args.get(0)?.as_str()?;
                let mode = if args.get(1)?.as_str()? == "move" {
                    MouseMode::Move
                } else {
                    MouseMode::Scroll
                };
                mouse.do_mouse_action(&mut key_press_state, key, mode);
            }
            _ => {}
        }

        Some(r#"{"status": "ok"}"#.to_string())
    }
}

fn main() {
    println!("Starting hintsd daemon (Rust)...");

    gtk::init().expect("Failed to initialize GTK");
    let display = gdk::Display::default().expect("Failed to get default display");
    let mut max_x = 0;
    let mut max_y = 0;

    for i in 0..display.n_monitors() {
        if let Some(monitor) = display.monitor(i) {
            let geo = monitor.geometry();
            max_x = max_x.max(geo.x() + geo.width());
            max_y = max_y.max(geo.y() + geo.height());
        }
    }

    if max_x == 0 || max_y == 0 {
        max_x = 1920;
        max_y = 1080;
    }

    println!("Detected screen size: {}x{}", max_x, max_y);

    let service = MouseService::new(max_x, max_y);

    println!("hintsd listening on {}", UNIX_DOMAIN_SOCKET_FILE);
    service.run();
}
