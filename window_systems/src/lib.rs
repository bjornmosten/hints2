use std::process::Command;
use x11rb::connection::Connection;

#[derive(Debug, Clone, Copy)]
pub enum WindowSystemType {
    X11,
    Wayland,
}

pub trait WindowSystem {
    fn window_system_name(&self) -> &str;
    fn focused_window_extents(&self) -> (i32, i32, i32, i32);
    fn focused_window_pid(&self) -> u32;
    fn focused_application_name(&self) -> &str;
    fn window_system_type(&self) -> WindowSystemType;
}

pub fn detect() -> Box<dyn WindowSystem> {
    let xdg_session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

    if xdg_session_type == "wayland" {
        let output = Command::new("ps")
            .args(["-e", "-o", "comm"])
            .output()
            .expect("Failed to run ps command");

        let processes = String::from_utf8_lossy(&output.stdout);
        let supported_wms = ["sway", "Hyprland", "plasmashell", "gnome-shell"];

        for process in processes.lines() {
            let process_lower = process.to_lowercase();
            for wm in &supported_wms {
                if process_lower == wm.to_lowercase() {
                    return create_window_system(wm);
                }
            }
        }
    }

    Box::new(X11Impl::new())
}

pub fn get_window_system(
    override_ws: Option<&str>,
) -> Result<Box<dyn WindowSystem>, WindowSystemError> {
    if let Some(ws) = override_ws {
        let ws_lower = ws.to_lowercase();
        match ws_lower.as_str() {
            "x11" => Ok(Box::new(X11Impl::new())),
            "sway" => Ok(Box::new(SwayImpl::new())),
            "hyprland" => Ok(Box::new(HyprlandImpl::new())),
            "plasmashell" => Ok(Box::new(PlasmashellImpl::new())),
            "gnome-shell" | "gnome" => Ok(Box::new(GnomeImpl::new())),
            _ => Err(WindowSystemError {
                message: format!("Unknown window system: {}", ws),
            }),
        }
    } else {
        Ok(detect())
    }
}

fn create_window_system(name: &str) -> Box<dyn WindowSystem> {
    match name.to_lowercase().as_str() {
        "sway" => Box::new(SwayImpl::new()),
        "hyprland" => Box::new(HyprlandImpl::new()),
        "plasmashell" => Box::new(PlasmashellImpl::new()),
        "gnome-shell" | "gnome" => Box::new(GnomeImpl::new()),
        _ => Box::new(X11Impl::new()),
    }
}

#[derive(Debug)]
pub struct WindowSystemError {
    pub message: String,
}

impl std::fmt::Display for WindowSystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WindowSystemError: {}", self.message)
    }
}

impl std::error::Error for WindowSystemError {}

pub struct X11Impl {
    connection: x11rb::xcb_ffi::XCBConnection,
    screen: usize,
}

impl X11Impl {
    pub fn new() -> Self {
        let (connection, screen) =
            x11rb::xcb_ffi::XCBConnection::connect(None).expect("Failed to connect to X11");
        Self { connection, screen }
    }
}

impl WindowSystem for X11Impl {
    fn window_system_name(&self) -> &str {
        "x11"
    }

    fn focused_window_extents(&self) -> (i32, i32, i32, i32) {
        use x11rb::protocol::xproto::{get_geometry, query_tree};

        let setup = self.connection.setup();
        let root = setup.roots[self.screen].root;

        let tree_reply = query_tree(&self.connection, root)
            .expect("Failed to query tree")
            .reply()
            .expect("Failed to get tree reply");

        let window = tree_reply.children.last().copied().unwrap_or(0);

        if window == 0 {
            return (0, 0, 0, 0);
        }

        let geom_reply = get_geometry(&self.connection, window)
            .expect("Failed to get geometry")
            .reply()
            .expect("Failed to get geometry reply");

        (
            geom_reply.x as i32,
            geom_reply.y as i32,
            geom_reply.width as i32,
            geom_reply.height as i32,
        )
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

pub struct SwayImpl {
    focused_window: SwayWindow,
}

#[derive(Debug, Clone)]
struct SwayWindow {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pid: u32,
    app_id: String,
}

impl SwayImpl {
    pub fn new() -> Self {
        let focused_window = Self::get_focused_window_from_sway_tree();
        Self { focused_window }
    }

    fn get_focused_window_from_sway_tree() -> SwayWindow {
        let output = Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .output()
            .expect("Failed to run swaymsg");

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse swaymsg JSON");

        let focused = Self::find_focused_node(&json);
        SwayWindow {
            x: focused["rect"]["x"].as_i64().unwrap_or(0) as i32,
            y: focused["rect"]["y"].as_i64().unwrap_or(0) as i32,
            width: focused["rect"]["width"].as_i64().unwrap_or(0) as i32,
            height: focused["rect"]["height"].as_i64().unwrap_or(0) as i32,
            pid: focused["pid"].as_u64().unwrap_or(0) as u32,
            app_id: focused["app_id"].as_str().unwrap_or("").to_string(),
        }
    }

    fn find_focused_node(node: &serde_json::Value) -> serde_json::Value {
        if node["focused"].as_bool().unwrap_or(false) {
            return node.clone();
        }
        if let Some(nodes) = node["nodes"].as_array() {
            for n in nodes {
                let found = Self::find_focused_node(n);
                if found["focused"].as_bool().unwrap_or(false) {
                    return found;
                }
            }
        }
        if let Some(floating_nodes) = node["floating_nodes"].as_array() {
            for n in floating_nodes {
                let found = Self::find_focused_node(n);
                if found["focused"].as_bool().unwrap_or(false) {
                    return found;
                }
            }
        }
        serde_json::Value::Null
    }
}

impl WindowSystem for SwayImpl {
    fn window_system_name(&self) -> &str {
        "sway"
    }

    fn focused_window_extents(&self) -> (i32, i32, i32, i32) {
        (
            self.focused_window.x,
            self.focused_window.y,
            self.focused_window.width,
            self.focused_window.height,
        )
    }

    fn focused_window_pid(&self) -> u32 {
        self.focused_window.pid
    }

    fn focused_application_name(&self) -> &str {
        &self.focused_window.app_id
    }

    fn window_system_type(&self) -> WindowSystemType {
        WindowSystemType::Wayland
    }
}

pub struct HyprlandImpl {
    focused_window: HyprlandWindow,
}

#[derive(Debug, Clone)]
struct HyprlandWindow {
    at: (i32, i32),
    size: (i32, i32),
    pid: u32,
    class: String,
}

impl HyprlandImpl {
    pub fn new() -> Self {
        let output = Command::new("hyprctl")
            .args(["activewindow", "-j"])
            .output()
            .expect("Failed to run hyprctl");

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse hyprctl JSON");

        let at = json["at"]
            .as_array()
            .map(|arr| {
                (
                    arr[0].as_i64().unwrap_or(0) as i32,
                    arr[1].as_i64().unwrap_or(0) as i32,
                )
            })
            .unwrap_or((0, 0));

        let size = json["size"]
            .as_array()
            .map(|arr| {
                (
                    arr[0].as_i64().unwrap_or(0) as i32,
                    arr[1].as_i64().unwrap_or(0) as i32,
                )
            })
            .unwrap_or((0, 0));

        HyprlandImpl {
            focused_window: HyprlandWindow {
                at,
                size,
                pid: json["pid"].as_u64().unwrap_or(0) as u32,
                class: json["class"].as_str().unwrap_or("").to_string(),
            },
        }
    }
}

impl WindowSystem for HyprlandImpl {
    fn window_system_name(&self) -> &str {
        "Hyprland"
    }

    fn focused_window_extents(&self) -> (i32, i32, i32, i32) {
        (
            self.focused_window.at.0,
            self.focused_window.at.1,
            self.focused_window.size.0,
            self.focused_window.size.1,
        )
    }

    fn focused_window_pid(&self) -> u32 {
        self.focused_window.pid
    }

    fn focused_application_name(&self) -> &str {
        &self.focused_window.class
    }

    fn window_system_type(&self) -> WindowSystemType {
        WindowSystemType::Wayland
    }
}

pub struct PlasmashellImpl {
    active_window: PlasmashellWindow,
}

#[derive(Debug, Clone)]
struct PlasmashellWindow {
    extents: (i32, i32, i32, i32),
    pid: u32,
    name: String,
}

impl PlasmashellImpl {
    pub fn new() -> Self {
        let active_window = Self::get_active_window();
        Self { active_window }
    }

    fn get_active_window() -> PlasmashellWindow {
        use std::time::Instant;
        let start_time = Instant::now();

        let _output = Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting.loadScript",
                "string:/dev/null",
            ])
            .output();

        let elapsed = start_time.elapsed();
        let since = format!("{} seconds ago", elapsed.as_secs());

        let output = Command::new("journalctl")
            .args(["_COMM=kwin_wayland", "--since", &since, "--output=cat"])
            .output();

        let journal_output = output
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let data: serde_json::Value =
            serde_json::from_str(&journal_output).unwrap_or(serde_json::json!({}));

        let extents_arr = data["extents"].as_array();
        PlasmashellWindow {
            extents: (
                extents_arr
                    .and_then(|a| a.get(0))
                    .map(|v| v.as_i64().unwrap_or(0) as i32)
                    .unwrap_or(0),
                extents_arr
                    .and_then(|a| a.get(1))
                    .map(|v| v.as_i64().unwrap_or(0) as i32)
                    .unwrap_or(0),
                extents_arr
                    .and_then(|a| a.get(2))
                    .map(|v| v.as_i64().unwrap_or(0) as i32)
                    .unwrap_or(0),
                extents_arr
                    .and_then(|a| a.get(3))
                    .map(|v| v.as_i64().unwrap_or(0) as i32)
                    .unwrap_or(0),
            ),
            pid: data["pid"].as_u64().unwrap_or(0) as u32,
            name: data["name"].as_str().unwrap_or("").to_string(),
        }
    }
}

impl WindowSystem for PlasmashellImpl {
    fn window_system_name(&self) -> &str {
        "plasmashell"
    }

    fn focused_window_extents(&self) -> (i32, i32, i32, i32) {
        self.active_window.extents
    }

    fn focused_window_pid(&self) -> u32 {
        self.active_window.pid
    }

    fn focused_application_name(&self) -> &str {
        &self.active_window.name
    }

    fn window_system_type(&self) -> WindowSystemType {
        WindowSystemType::Wayland
    }
}

pub struct GnomeImpl {
    window_info: GnomeWindowInfo,
}

#[derive(Debug, Clone)]
struct GnomeWindowInfo {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pid: u32,
    name: String,
    monitor: i32,
}

impl GnomeImpl {
    pub fn new() -> Self {
        let window_info = Self::get_focused_window_info();
        Self { window_info }
    }

    fn get_focused_window_info() -> GnomeWindowInfo {
        GnomeWindowInfo {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            pid: 0,
            name: String::new(),
            monitor: 0,
        }
    }
}

impl WindowSystem for GnomeImpl {
    fn window_system_name(&self) -> &str {
        "gnome"
    }

    fn focused_window_extents(&self) -> (i32, i32, i32, i32) {
        (
            self.window_info.x,
            self.window_info.y,
            self.window_info.width,
            self.window_info.height,
        )
    }

    fn focused_window_pid(&self) -> u32 {
        self.window_info.pid
    }

    fn focused_application_name(&self) -> &str {
        &self.window_info.name
    }

    fn window_system_type(&self) -> WindowSystemType {
        WindowSystemType::Wayland
    }
}
