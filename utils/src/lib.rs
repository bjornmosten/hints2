use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const UNIX_DOMAIN_SOCKET_FILE: &str = "/tmp/hints.socket";
pub const CONFIG_PATH: &str = "~/.config/hints/config.json";

#[derive(Debug, Clone)]
pub struct Child {
    pub absolute_position: (f64, f64),
    pub relative_position: (f64, f64),
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub hints: HintsConfig,
    #[serde(default)]
    pub backends: BackendsConfig,
    #[serde(default = "default_alphabet")]
    pub alphabet: String,
    #[serde(default = "default_mouse_move_left")]
    pub mouse_move_left: String,
    #[serde(default = "default_mouse_move_right")]
    pub mouse_move_right: String,
    #[serde(default = "default_mouse_move_up")]
    pub mouse_move_up: String,
    #[serde(default = "default_mouse_move_down")]
    pub mouse_move_down: String,
    #[serde(default = "default_mouse_scroll_left")]
    pub mouse_scroll_left: String,
    #[serde(default = "default_mouse_scroll_right")]
    pub mouse_scroll_right: String,
    #[serde(default = "default_mouse_scroll_up")]
    pub mouse_scroll_up: String,
    #[serde(default = "default_mouse_scroll_down")]
    pub mouse_scroll_down: String,
    #[serde(default = "default_mouse_move_pixel")]
    pub mouse_move_pixel: u32,
    #[serde(default = "default_mouse_move_pixel_sensitivity")]
    pub mouse_move_pixel_sensitivity: u32,
    #[serde(default = "default_mouse_move_rampup_time")]
    pub mouse_move_rampup_time: f64,
    #[serde(default = "default_mouse_scroll_pixel")]
    pub mouse_scroll_pixel: u32,
    #[serde(default = "default_mouse_scroll_pixel_sensitivity")]
    pub mouse_scroll_pixel_sensitivity: u32,
    #[serde(default = "default_mouse_scroll_rampup_time")]
    pub mouse_scroll_rampup_time: f64,
    #[serde(default)]
    pub exit_key: u32,
    #[serde(default)]
    pub hover_modifier: u32,
    #[serde(default)]
    pub grab_modifier: u32,
    #[serde(default)]
    pub overlay_x_offset: i32,
    #[serde(default)]
    pub overlay_y_offset: i32,
    #[serde(default)]
    pub window_system: String,
}

fn default_alphabet() -> String {
    "asdfgqwertzxcvbhjklyuiopnm".to_string()
}

fn default_mouse_move_left() -> String {
    "h".to_string()
}
fn default_mouse_move_right() -> String {
    "l".to_string()
}
fn default_mouse_move_up() -> String {
    "k".to_string()
}
fn default_mouse_move_down() -> String {
    "j".to_string()
}
fn default_mouse_scroll_left() -> String {
    "h".to_string()
}
fn default_mouse_scroll_right() -> String {
    "l".to_string()
}
fn default_mouse_scroll_up() -> String {
    "k".to_string()
}
fn default_mouse_scroll_down() -> String {
    "j".to_string()
}
fn default_mouse_move_pixel() -> u32 {
    10
}
fn default_mouse_move_pixel_sensitivity() -> u32 {
    10
}
fn default_mouse_move_rampup_time() -> f64 {
    0.5
}
fn default_mouse_scroll_pixel() -> u32 {
    5
}
fn default_mouse_scroll_pixel_sensitivity() -> u32 {
    5
}
fn default_mouse_scroll_rampup_time() -> f64 {
    0.5
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hints: HintsConfig::default(),
            backends: BackendsConfig::default(),
            alphabet: default_alphabet(),
            mouse_move_left: default_mouse_move_left(),
            mouse_move_right: default_mouse_move_right(),
            mouse_move_up: default_mouse_move_up(),
            mouse_move_down: default_mouse_move_down(),
            mouse_scroll_left: default_mouse_scroll_left(),
            mouse_scroll_right: default_mouse_scroll_right(),
            mouse_scroll_up: default_mouse_scroll_up(),
            mouse_scroll_down: default_mouse_scroll_down(),
            mouse_move_pixel: default_mouse_move_pixel(),
            mouse_move_pixel_sensitivity: default_mouse_move_pixel_sensitivity(),
            mouse_move_rampup_time: default_mouse_move_rampup_time(),
            mouse_scroll_pixel: default_mouse_scroll_pixel(),
            mouse_scroll_pixel_sensitivity: default_mouse_scroll_pixel_sensitivity(),
            mouse_scroll_rampup_time: default_mouse_scroll_rampup_time(),
            exit_key: 0,
            hover_modifier: 0,
            grab_modifier: 0,
            overlay_x_offset: 0,
            overlay_y_offset: 0,
            window_system: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HintsConfig {
    #[serde(default = "default_hint_height")]
    pub hint_height: u32,
    #[serde(default = "default_hint_width_padding")]
    pub hint_width_padding: u32,
    #[serde(default = "default_hint_font_size")]
    pub hint_font_size: u32,
    #[serde(default = "default_hint_font_face")]
    pub hint_font_face: String,
    #[serde(default = "default_hint_font_color")]
    pub hint_font_r: f64,
    #[serde(default = "default_hint_font_color")]
    pub hint_font_g: f64,
    #[serde(default = "default_hint_font_color")]
    pub hint_font_b: f64,
    #[serde(default = "default_hint_font_alpha")]
    pub hint_font_a: f64,
    #[serde(default = "default_hint_pressed_font_r")]
    pub hint_pressed_font_r: f64,
    #[serde(default = "default_hint_pressed_font_g")]
    pub hint_pressed_font_g: f64,
    #[serde(default = "default_hint_pressed_font_b")]
    pub hint_pressed_font_b: f64,
    #[serde(default = "default_hint_font_alpha")]
    pub hint_pressed_font_a: f64,
    #[serde(default = "default_hint_upercase")]
    pub hint_upercase: bool,
    #[serde(default = "default_hint_background_r")]
    pub hint_background_r: f64,
    #[serde(default = "default_hint_background_g")]
    pub hint_background_g: f64,
    #[serde(default = "default_hint_background_b")]
    pub hint_background_b: f64,
    #[serde(default = "default_hint_background_alpha")]
    pub hint_background_a: f64,
}

fn default_hint_height() -> u32 {
    30
}
fn default_hint_width_padding() -> u32 {
    10
}
fn default_hint_font_size() -> u32 {
    15
}
fn default_hint_font_face() -> String {
    "Sans".to_string()
}
fn default_hint_font_color() -> f64 {
    0.0
}
fn default_hint_font_alpha() -> f64 {
    1.0
}
fn default_hint_pressed_font_r() -> f64 {
    0.7
}
fn default_hint_pressed_font_g() -> f64 {
    0.7
}
fn default_hint_pressed_font_b() -> f64 {
    0.4
}
fn default_hint_upercase() -> bool {
    true
}
fn default_hint_background_r() -> f64 {
    1.0
}
fn default_hint_background_g() -> f64 {
    1.0
}
fn default_hint_background_b() -> f64 {
    0.5
}
fn default_hint_background_alpha() -> f64 {
    0.8
}

impl Default for HintsConfig {
    fn default() -> Self {
        Self {
            hint_height: default_hint_height(),
            hint_width_padding: default_hint_width_padding(),
            hint_font_size: default_hint_font_size(),
            hint_font_face: default_hint_font_face(),
            hint_font_r: default_hint_font_color(),
            hint_font_g: default_hint_font_color(),
            hint_font_b: default_hint_font_color(),
            hint_font_a: default_hint_font_alpha(),
            hint_pressed_font_r: default_hint_pressed_font_r(),
            hint_pressed_font_g: default_hint_pressed_font_g(),
            hint_pressed_font_b: default_hint_pressed_font_b(),
            hint_pressed_font_a: default_hint_font_alpha(),
            hint_upercase: default_hint_upercase(),
            hint_background_r: default_hint_background_r(),
            hint_background_g: default_hint_background_g(),
            hint_background_b: default_hint_background_b(),
            hint_background_a: default_hint_background_alpha(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendsConfig {
    #[serde(default = "default_backends_enable")]
    pub enable: Vec<String>,
    #[serde(default)]
    pub atspi: AtspiConfig,
    #[serde(default)]
    pub opencv: OpenCvConfig,
}

fn default_backends_enable() -> Vec<String> {
    vec!["atspi".to_string(), "opencv".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtspiConfig {
    #[serde(default)]
    pub application_rules: HashMap<String, ApplicationRule>,
}

impl Default for AtspiConfig {
    fn default() -> Self {
        Self {
            application_rules: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCvConfig {
    #[serde(default)]
    pub application_rules: HashMap<String, OpenCvApplicationRule>,
}

impl Default for OpenCvConfig {
    fn default() -> Self {
        Self {
            application_rules: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationRule {
    #[serde(default = "default_scale_factor")]
    pub scale_factor: f64,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub states_match_type: String,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
    #[serde(default)]
    pub attributes_match_type: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub roles_match_type: String,
}

fn default_scale_factor() -> f64 {
    1.0
}

impl Default for ApplicationRule {
    fn default() -> Self {
        Self {
            scale_factor: default_scale_factor(),
            states: Vec::new(),
            states_match_type: "all".to_string(),
            attributes: HashMap::new(),
            attributes_match_type: "all".to_string(),
            roles: Vec::new(),
            roles_match_type: "none".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCvApplicationRule {
    #[serde(default = "default_kernel_size")]
    pub kernel_size: u32,
    #[serde(default = "default_canny_min_val")]
    pub canny_min_val: u32,
    #[serde(default = "default_canny_max_val")]
    pub canny_max_val: u32,
}

fn default_kernel_size() -> u32 {
    6
}
fn default_canny_min_val() -> u32 {
    100
}
fn default_canny_max_val() -> u32 {
    200
}

impl Default for OpenCvApplicationRule {
    fn default() -> Self {
        Self {
            kernel_size: default_kernel_size(),
            canny_min_val: default_canny_min_val(),
            canny_max_val: default_canny_max_val(),
        }
    }
}

pub fn load_config() -> Config {
    let config_path = expand_path(&CONFIG_PATH);

    if let Ok(contents) = fs::read_to_string(&config_path) {
        if let Ok(user_config) = serde_json::from_str::<Config>(&contents) {
            return merge_config(&Config::default(), &user_config);
        }
    }

    Config::default()
}

fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(path.replacen("~", &home, 1));
        }
    }
    PathBuf::from(path)
}

fn merge_config(default_config: &Config, user_config: &Config) -> Config {
    let mut merged = default_config.clone();

    merged.hints = merge_hints_config(&default_config.hints, &user_config.hints);
    merged.backends = merge_backends_config(&default_config.backends, &user_config.backends);

    if user_config.alphabet != default_config.alphabet {
        merged.alphabet = user_config.alphabet.clone();
    }
    if user_config.mouse_move_left != default_config.mouse_move_left {
        merged.mouse_move_left = user_config.mouse_move_left.clone();
    }
    if user_config.mouse_move_right != default_config.mouse_move_right {
        merged.mouse_move_right = user_config.mouse_move_right.clone();
    }
    if user_config.mouse_move_up != default_config.mouse_move_up {
        merged.mouse_move_up = user_config.mouse_move_up.clone();
    }
    if user_config.mouse_move_down != default_config.mouse_move_down {
        merged.mouse_move_down = user_config.mouse_move_down.clone();
    }
    if user_config.mouse_scroll_left != default_config.mouse_scroll_left {
        merged.mouse_scroll_left = user_config.mouse_scroll_left.clone();
    }
    if user_config.mouse_scroll_right != default_config.mouse_scroll_right {
        merged.mouse_scroll_right = user_config.mouse_scroll_right.clone();
    }
    if user_config.mouse_scroll_up != default_config.mouse_scroll_up {
        merged.mouse_scroll_up = user_config.mouse_scroll_up.clone();
    }
    if user_config.mouse_scroll_down != default_config.mouse_scroll_down {
        merged.mouse_scroll_down = user_config.mouse_scroll_down.clone();
    }
    if user_config.mouse_move_pixel != default_config.mouse_move_pixel {
        merged.mouse_move_pixel = user_config.mouse_move_pixel;
    }
    if user_config.mouse_move_pixel_sensitivity != default_config.mouse_move_pixel_sensitivity {
        merged.mouse_move_pixel_sensitivity = user_config.mouse_move_pixel_sensitivity;
    }
    if user_config.mouse_move_rampup_time != default_config.mouse_move_rampup_time {
        merged.mouse_move_rampup_time = user_config.mouse_move_rampup_time;
    }
    if user_config.mouse_scroll_pixel != default_config.mouse_scroll_pixel {
        merged.mouse_scroll_pixel = user_config.mouse_scroll_pixel;
    }
    if user_config.mouse_scroll_pixel_sensitivity != default_config.mouse_scroll_pixel_sensitivity {
        merged.mouse_scroll_pixel_sensitivity = user_config.mouse_scroll_pixel_sensitivity;
    }
    if user_config.mouse_scroll_rampup_time != default_config.mouse_scroll_rampup_time {
        merged.mouse_scroll_rampup_time = user_config.mouse_scroll_rampup_time;
    }
    if user_config.exit_key != 0 {
        merged.exit_key = user_config.exit_key;
    }
    if user_config.hover_modifier != 0 {
        merged.hover_modifier = user_config.hover_modifier;
    }
    if user_config.grab_modifier != 0 {
        merged.grab_modifier = user_config.grab_modifier;
    }
    if user_config.overlay_x_offset != 0 {
        merged.overlay_x_offset = user_config.overlay_x_offset;
    }
    if user_config.overlay_y_offset != 0 {
        merged.overlay_y_offset = user_config.overlay_y_offset;
    }
    if !user_config.window_system.is_empty() {
        merged.window_system = user_config.window_system.clone();
    }

    merged
}

fn merge_hints_config(default_hints: &HintsConfig, user_hints: &HintsConfig) -> HintsConfig {
    let mut merged = default_hints.clone();

    if user_hints.hint_height != default_hints.hint_height {
        merged.hint_height = user_hints.hint_height;
    }
    if user_hints.hint_width_padding != default_hints.hint_width_padding {
        merged.hint_width_padding = user_hints.hint_width_padding;
    }
    if user_hints.hint_font_size != default_hints.hint_font_size {
        merged.hint_font_size = user_hints.hint_font_size;
    }
    if user_hints.hint_font_face != default_hints.hint_font_face {
        merged.hint_font_face = user_hints.hint_font_face.clone();
    }
    if user_hints.hint_font_r != default_hints.hint_font_r {
        merged.hint_font_r = user_hints.hint_font_r;
    }
    if user_hints.hint_font_g != default_hints.hint_font_g {
        merged.hint_font_g = user_hints.hint_font_g;
    }
    if user_hints.hint_font_b != default_hints.hint_font_b {
        merged.hint_font_b = user_hints.hint_font_b;
    }
    if user_hints.hint_font_a != default_hints.hint_font_a {
        merged.hint_font_a = user_hints.hint_font_a;
    }
    if user_hints.hint_pressed_font_r != default_hints.hint_pressed_font_r {
        merged.hint_pressed_font_r = user_hints.hint_pressed_font_r;
    }
    if user_hints.hint_pressed_font_g != default_hints.hint_pressed_font_g {
        merged.hint_pressed_font_g = user_hints.hint_pressed_font_g;
    }
    if user_hints.hint_pressed_font_b != default_hints.hint_pressed_font_b {
        merged.hint_pressed_font_b = user_hints.hint_pressed_font_b;
    }
    if user_hints.hint_pressed_font_a != default_hints.hint_pressed_font_a {
        merged.hint_pressed_font_a = user_hints.hint_pressed_font_a;
    }
    if user_hints.hint_upercase != default_hints.hint_upercase {
        merged.hint_upercase = user_hints.hint_upercase;
    }
    if user_hints.hint_background_r != default_hints.hint_background_r {
        merged.hint_background_r = user_hints.hint_background_r;
    }
    if user_hints.hint_background_g != default_hints.hint_background_g {
        merged.hint_background_g = user_hints.hint_background_g;
    }
    if user_hints.hint_background_b != default_hints.hint_background_b {
        merged.hint_background_b = user_hints.hint_background_b;
    }
    if user_hints.hint_background_a != default_hints.hint_background_a {
        merged.hint_background_a = user_hints.hint_background_a;
    }

    merged
}

fn merge_backends_config(
    default_backends: &BackendsConfig,
    user_backends: &BackendsConfig,
) -> BackendsConfig {
    let mut merged = default_backends.clone();

    if !user_backends.enable.is_empty() && user_backends.enable != default_backends.enable {
        merged.enable = user_backends.enable.clone();
    }

    merged
}

pub fn get_hints(children: &[Child], alphabet: &str) -> HashMap<String, Child> {
    let n = children.len();
    if n == 0 {
        return HashMap::new();
    }

    let alphabet_len = alphabet.chars().count();
    let hint_length = ((n as f64).log(alphabet_len as f64).ceil()) as usize;

    let chars: Vec<char> = alphabet.chars().collect();
    let mut hints = HashMap::new();

    for (index, child) in children.iter().enumerate() {
        let mut label = String::new();
        let mut remaining = index;

        for _ in 0..hint_length {
            let char_index = remaining % alphabet_len;
            label.push(chars[char_index]);
            remaining /= alphabet_len;
        }

        hints.insert(label, child.clone());
    }

    hints
}

#[derive(Debug)]
pub struct AccessibleChildrenNotFound {
    pub message: String,
}

impl Error for AccessibleChildrenNotFound {}

impl std::fmt::Display for AccessibleChildrenNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AccessibleChildrenNotFound: {}", self.message)
    }
}

#[derive(Debug)]
pub struct CouldNotFindAccessibleWindow {
    pub message: String,
}

impl Error for CouldNotFindAccessibleWindow {}

impl std::fmt::Display for CouldNotFindAccessibleWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CouldNotFindAccessibleWindow: {}", self.message)
    }
}

#[derive(Debug)]
pub struct WindowSystemNotSupported {
    pub message: String,
}

impl Error for WindowSystemNotSupported {}

impl std::fmt::Display for WindowSystemNotSupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WindowSystemNotSupported: {}", self.message)
    }
}

#[derive(Debug)]
pub struct CouldNotCommunicateWithMouseService {
    pub message: String,
}

impl Error for CouldNotCommunicateWithMouseService {}

impl std::fmt::Display for CouldNotCommunicateWithMouseService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CouldNotCommunicateWithMouseService: {}", self.message)
    }
}

pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;

#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
}

impl MouseButton {
    pub fn to_evdev_code(&self) -> u32 {
        match self {
            MouseButton::Left => BTN_LEFT,
            MouseButton::Right => BTN_RIGHT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MouseButtonState {
    Down = 1,
    Up = 0,
}

#[derive(Debug, Clone, Copy)]
pub enum MouseMode {
    Move,
    Scroll,
}

#[derive(Debug, Clone, Copy)]
pub enum WindowSystemType {
    X11,
    Wayland,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_hints() {
        let children = vec![
            Child {
                absolute_position: (0.0, 0.0),
                relative_position: (0.0, 0.0),
                width: 100.0,
                height: 50.0,
            },
            Child {
                absolute_position: (100.0, 0.0),
                relative_position: (0.5, 0.0),
                width: 100.0,
                height: 50.0,
            },
            Child {
                absolute_position: (0.0, 50.0),
                relative_position: (0.0, 0.5),
                width: 100.0,
                height: 50.0,
            },
        ];

        let hints = get_hints(&children, "asdf");
        assert_eq!(hints.len(), 3);
    }

    #[test]
    fn test_config_load() {
        let config = Config::default();
        assert_eq!(config.alphabet, "asdfgqwertzxcvbhjklyuiopnm");
        assert_eq!(config.mouse_move_pixel, 10);
    }
}
