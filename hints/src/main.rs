use clap::{Parser, ValueEnum};
use log::{info, LevelFilter};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use utils::{Config, UNIX_DOMAIN_SOCKET_FILE};

mod interceptor;
mod overlay;
mod setup;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "hint")]
    mode: Mode,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long)]
    setup: bool,
}

#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    Hint,
    Scroll,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Hint
    }
}

struct MouseClient {
    socket_path: String,
}

impl MouseClient {
    fn new() -> Self {
        Self {
            socket_path: UNIX_DOMAIN_SOCKET_FILE.to_string(),
        }
    }

    fn send_message(
        &self,
        method: &str,
        args: Vec<serde_json::Value>,
        kwargs: serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let mut stream = StdUnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to socket: {}", e))?;

        let message = serde_json::json!({
            "method": method,
            "args": args,
            "kwargs": kwargs,
        });

        stream
            .write_all(message.to_string().as_bytes())
            .map_err(|e| format!("Failed to send message: {}", e))?;

        let mut buffer = [0u8; 1024];
        let n = stream
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read response: {}", e))?;

        let response: serde_json::Value = serde_json::from_slice(&buffer[..n])
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(response)
    }

    fn click(
        &self,
        x: f64,
        y: f64,
        button: &str,
        button_states: &[i64],
        repeat: u32,
        absolute: bool,
    ) -> Result<(), String> {
        let args = serde_json::json!([x, y, button, button_states, repeat]);
        let mut kwargs = serde_json::Map::new();
        kwargs.insert("absolute".to_string(), serde_json::json!(absolute));
        self.send_message("click", args.as_array().unwrap().to_vec(), kwargs)?;
        Ok(())
    }

    fn move_(&self, x: f64, y: f64, absolute: bool) -> Result<(), String> {
        let args = serde_json::json!([x, y]);
        let mut kwargs = serde_json::Map::new();
        kwargs.insert("absolute".to_string(), serde_json::json!(absolute));
        self.send_message("move", args.as_array().unwrap().to_vec(), kwargs)?;
        Ok(())
    }

    fn scroll(&self, x: i32, y: i32) -> Result<(), String> {
        let args = serde_json::json!([x, y]);
        let kwargs = serde_json::Map::new();
        self.send_message("scroll", args.as_array().unwrap().to_vec(), kwargs)?;
        Ok(())
    }

    fn do_mouse_action(
        &self,
        key_press_state: Option<&str>,
        key: &str,
        mode: &str,
    ) -> Result<(), String> {
        let args = serde_json::json!([key, mode]);
        let mut kwargs = serde_json::Map::new();
        if let Some(state) = key_press_state {
            kwargs.insert("key_press_state".to_string(), serde_json::json!(state));
        }
        self.send_message("do_mouse_action", args.as_array().unwrap().to_vec(), kwargs)?;
        Ok(())
    }
}

fn hint_mode(config: &Config) {
    info!("Running in hint mode");
}

fn scroll_mode(config: &Config) {
    info!("Running in scroll mode");
}

fn setup_mode() {
    setup::run_guided_setup();
}

fn main() {
    let args = Args::parse();

    if args.verbose >= 1 {
        env_logger::Builder::from_default_env()
            .filter_level(LevelFilter::Debug)
            .init();
    } else {
        env_logger::Builder::from_default_env()
            .filter_level(LevelFilter::Info)
            .init();
    }

    let config = utils::load_config();

    if args.setup {
        setup_mode();
        return;
    }

    match args.mode {
        Mode::Hint => hint_mode(&config),
        Mode::Scroll => scroll_mode(&config),
    }
}
