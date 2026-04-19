use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use gtk::prelude::*;
use log::{debug, info, LevelFilter};

use backends::get_backends;
use hints::interceptor::InterceptorWindow;
use hints::mouse_client::MouseClient;
use hints::overlay::{MouseAction, OverlayWindow};
use utils::Config;
use window_systems::{get_window_system, WindowSystem, WindowSystemType};

mod setup;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "hint")]
    mode: Mode,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long)]
    setup: bool,

    /// Override `backends.enable` with a comma-separated list (e.g. `yolo`).
    #[arg(short = 'b', long)]
    backend: Option<String>,

    /// Run the selected backend once, print a timing breakdown, and exit
    /// without opening any GTK overlay or talking to the mouse service.
    #[arg(long)]
    benchmark: bool,

    /// Feed a PNG/JPG from disk into the backend instead of capturing the
    /// screen. Lets benchmarks run without a display session.
    #[arg(long)]
    benchmark_image: Option<PathBuf>,

    /// Override the model path used by the yolo backend (defaults to
    /// `backends.yolo.model_path` in the loaded config).
    #[arg(short = 'm', long)]
    model: Option<PathBuf>,
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

/// Positions an overlay/interceptor window on screen.
///
/// On X11 and GNOME-on-Wayland (with its extension) a plain `window.move_()`
/// call is sufficient. On other Wayland compositors the `gtk-layer-shell`
/// protocol would normally be used so that unprivileged clients can anchor
/// themselves to a monitor edge; see TODO below.
fn configure_window_placement(ws: &dyn WindowSystem, window: &gtk::Window, x: i32, y: i32) {
    match ws.window_system_name() {
        "gnome" => {
            // TODO: call GNOME shell extension via D-Bus to register this as an
            // overlay (see `hints/gnome_overlay.init_overlay_window` in the
            // Python reference). For now just position the window directly.
            window.move_(x, y);
        }
        _ if matches!(ws.window_system_type(), WindowSystemType::Wayland) => {
            // TODO: use gtk-layer-shell 0.8 once the `gtk-layer-shell-0`
            // system library is available in this build environment. For now
            // fall back to a plain move, which works on X11 and on GNOME's
            // Wayland session via the shell extension above.
            window.move_(x, y);
        }
        _ => {
            window.move_(x, y);
        }
    }
}

fn perform_action(action: &MouseAction, ws: &dyn WindowSystem, config: &Config, is_wayland: bool) {
    let client = MouseClient::new();
    match action.action.as_str() {
        "click" => {
            if let Err(e) = client.click(action.x, action.y, &action.button, &[1, 0], action.repeat, true) {
                debug!("click failed: {}", e);
            }
        }
        "hover" => {
            if let Err(e) = client.click(action.x, action.y, "left", &[], 1, true) {
                debug!("hover failed: {}", e);
            }
        }
        "grab" => {
            if let Err(e) = client.click(action.x, action.y, "left", &[1], 1, true) {
                debug!("grab press failed: {}", e);
            }
            let (x, y, _, _) = ws.focused_window_extents();
            let interceptor = InterceptorWindow::new(x, y, 1, 1, "grab".to_string(), config.clone(), is_wayland);
            configure_window_placement(ws, interceptor.window(), x, y);
            interceptor.window().show_all();
            gtk::main();
        }
        other => {
            debug!("unknown action: {}", other);
        }
    }
}

fn hint_mode(config: &Config, ws: &dyn WindowSystem) {
    info!("Running in hint mode");
    let is_wayland = matches!(ws.window_system_type(), WindowSystemType::Wayland);
    let (x, y, w, h) = ws.focused_window_extents();

    for (_kind, mut backend) in get_backends(config, ws) {
        match backend.get_children() {
            Ok(children) if !children.is_empty() => {
                let hints = utils::get_hints(&children, &config.alphabet);
                let overlay_x = x + config.overlay_x_offset;
                let overlay_y = y + config.overlay_y_offset;
                let overlay = OverlayWindow::new(
                    overlay_x,
                    overlay_y,
                    w,
                    h,
                    config.clone(),
                    hints,
                    is_wayland,
                );
                configure_window_placement(ws, overlay.window(), overlay_x, overlay_y);
                overlay.window().show_all();
                gtk::main();

                if let Some(action) = overlay.take_mouse_action() {
                    perform_action(&action, ws, config, is_wayland);
                }
                return;
            }
            Ok(_) => debug!("backend returned empty children; trying next"),
            Err(e) => debug!("backend failed: {}", e),
        }
    }
}

fn scroll_mode(config: &Config, ws: &dyn WindowSystem) {
    info!("Running in scroll mode");
    let is_wayland = matches!(ws.window_system_type(), WindowSystemType::Wayland);
    let interceptor = InterceptorWindow::new(0, 0, 1, 1, "scroll".to_string(), config.clone(), is_wayland);
    configure_window_placement(ws, interceptor.window(), 0, 0);
    interceptor.window().show_all();
    gtk::main();
}

/// A minimal [`WindowSystem`] stub that returns zeros everywhere. Used only
/// for `--benchmark --benchmark-image` so that no display server is needed.
struct HeadlessWindowSystem;
impl WindowSystem for HeadlessWindowSystem {
    fn window_system_name(&self) -> &str {
        "headless"
    }
    fn focused_window_extents(&self) -> (i32, i32, i32, i32) {
        (0, 0, 0, 0)
    }
    fn focused_window_pid(&self) -> u32 {
        0
    }
    fn focused_application_name(&self) -> &str {
        ""
    }
    fn window_system_type(&self) -> WindowSystemType {
        WindowSystemType::X11
    }
}

fn benchmark_mode(
    config: &Config,
    backend_name: &str,
    image: Option<&std::path::Path>,
    model: Option<&std::path::Path>,
) -> i32 {
    // Only the yolo backend supports the benchmark path right now because
    // it's the only one we can exercise without a live X/Wayland session.
    if backend_name != "yolo" {
        eprintln!("--benchmark only supports --backend yolo");
        return 2;
    }

    let ws = HeadlessWindowSystem;
    let mut backend =
        match backends::yolo::YoloBackend::new_with_overrides(config, &ws, model, None) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("failed to construct yolo backend: {}", e);
                return 1;
            }
        };
    if let Some(path) = image {
        backend.frame_source = backends::yolo::FrameSource::File(path.to_path_buf());
    }

    match backend.run_timed() {
        Ok((children, t)) => {
            println!("yolo benchmark");
            println!("  detections     : {}", t.detections);
            for (i, child) in children.iter().enumerate() {
                println!(
                    "    {:2}: x={:>4.0}, y={:>4.0}, w={:>4.0}, h={:>4.0}",
                    i,
                    child.relative_position.0,
                    child.relative_position.1,
                    child.width,
                    child.height
                );
            }
            println!("  screenshot_ms  : {:.2}", t.screenshot_ms);
            println!("  preprocess_ms  : {:.2}", t.preprocess_ms);
            println!("  inference_ms   : {:.2}", t.inference_ms);
            println!("  postprocess_ms : {:.2}", t.postprocess_ms);
            println!("  total_ms       : {:.2}", t.total_ms);
            0
        }
        Err(e) => {
            eprintln!("benchmark failed: {}", e);
            1
        }
    }
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

    let mut config = utils::load_config();

    if args.setup {
        setup::run_guided_setup();
        return;
    }

    // Apply the `--backend` override so the rest of the pipeline sees it.
    if let Some(ref b) = args.backend {
        config.backends.enable = b.split(',').map(|s| s.trim().to_string()).collect();
    }

    if args.benchmark {
        let selected = args
            .backend
            .clone()
            .or_else(|| config.backends.enable.first().cloned())
            .unwrap_or_else(|| "yolo".to_string());
        let code = benchmark_mode(
            &config,
            &selected,
            args.benchmark_image.as_deref(),
            args.model.as_deref(),
        );
        std::process::exit(code);
    }

    gtk::init().expect("failed to init gtk");

    let override_ws = if config.window_system.is_empty() {
        None
    } else {
        Some(config.window_system.as_str())
    };
    let ws = match get_window_system(override_ws) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    match args.mode {
        Mode::Hint => hint_mode(&config, &*ws),
        Mode::Scroll => scroll_mode(&config, &*ws),
    }
}
