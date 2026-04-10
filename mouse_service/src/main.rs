use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use utils::{MouseButton, MouseButtonState, MouseMode, UNIX_DOMAIN_SOCKET_FILE};

const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_HWHEEL: u16 = 0x06;
const REL_WHEEL: u16 = 0x08;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

pub struct Mouse {
    screen_width: i32,
    screen_height: i32,
    write_pause: Duration,
}

impl Mouse {
    pub fn new(screen_width: i32, screen_height: i32) -> Self {
        Self {
            screen_width,
            screen_height,
            write_pause: Duration::from_millis(10),
        }
    }

    pub fn scroll(&mut self, x: i32, y: i32) {
        println!("scroll: x={}, y={}", x, y);
        thread::sleep(self.write_pause);
    }

    pub fn move_mouse(&mut self, x: i32, y: i32, absolute: bool) {
        println!("move_mouse: x={}, y={}, absolute={}", x, y, absolute);
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
        println!(
            "click: x={}, y={}, button={:?}, states={:?}, repeat={}, absolute={}",
            x, y, button, button_states, repeat, absolute
        );
        thread::sleep(self.write_pause);
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

    let screen_width = 1920;
    let screen_height = 1080;

    let service = MouseService::new(screen_width, screen_height);

    println!("hintsd listening on {}", UNIX_DOMAIN_SOCKET_FILE);
    service.run();
}
