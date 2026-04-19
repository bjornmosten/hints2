//! Backends for hints2: AT-SPI accessibility-tree and OpenCV edge-detection.
//!
//! Ported from the reference Python implementations at
//! `hints/backends/atspi.py` and `hints/backends/opencv.py`.

pub mod yolo;

use std::collections::HashMap;

use atspi::connection::AccessibilityConnection;
use atspi::proxy::{
    accessible::AccessibleProxy,
    application::ApplicationProxy,
    collection::CollectionProxy,
    component::ComponentProxy,
};
use atspi::proxy::zbus;
use atspi::{CoordType, MatchType, Role, SortOrder, State, StateSet};
use image::GrayImage;
use imageproc::{
    contours::find_contours,
    distance_transform::Norm,
    edges::canny,
    morphology::dilate,
};
use tokio::runtime::Runtime;
use utils::{
    AccessibleChildrenNotFound, ApplicationRule, Child, Config, OpenCvApplicationRule,
};
use window_systems::{WindowSystem, WindowSystemType};

/// A resolved per-application rule feeding both backends.
#[derive(Debug, Clone)]
pub struct ApplicationRules {
    pub scale_factor: f64,
    pub opencv_kernel_size: u32,
    pub opencv_canny_min_val: u32,
    pub opencv_canny_max_val: u32,
}

impl Default for ApplicationRules {
    fn default() -> Self {
        Self {
            scale_factor: 1.0,
            opencv_kernel_size: 6,
            opencv_canny_min_val: 100,
            opencv_canny_max_val: 200,
        }
    }
}

pub trait Backend {
    fn get_children(&mut self) -> Result<Vec<Child>, AccessibleChildrenNotFound>;
    fn get_application_rules(&self) -> ApplicationRules;
}

#[derive(Debug, Clone, Copy)]
pub enum BackendType {
    Atspi,
    OpenCV,
    Yolo,
}

// -----------------------------------------------------------------------------
// Rule-string parsing helpers
// -----------------------------------------------------------------------------

fn match_type_from_str(s: &str) -> MatchType {
    match s.to_ascii_lowercase().as_str() {
        "all" => MatchType::All,
        "any" => MatchType::Any,
        "none" | "na" | "n/a" => MatchType::NA,
        "empty" => MatchType::Empty,
        _ => MatchType::Invalid,
    }
}

fn state_from_config_str(s: &str) -> Option<State> {
    // The `State::from(&str)` impl expects kebab-case; normalize common variants.
    let normalized: String = s
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c == '_' || c == ' ' { '-' } else { c })
        .collect();
    let state = State::from(normalized.as_str());
    if matches!(state, State::Invalid) && normalized != "invalid" {
        None
    } else {
        Some(state)
    }
}

/// Try hard to match a configuration string to a [`Role`] variant. Accepts
/// the UPPER_SNAKE_CASE used by libatspi (e.g. `PUSH_BUTTON`) and the
/// lowercase human form returned by [`Role::name`] (e.g. `"push button"`).
fn role_from_config_str(s: &str) -> Option<Role> {
    let target = s
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-'], " ");
    // Role is `#[repr(u32)]` with a contiguous block of variants (currently
    // ~130); a bounded scan is simple and cheap.
    for code in 0u32..256 {
        if let Ok(role) = Role::try_from(code) {
            if role.name().eq_ignore_ascii_case(target.as_str()) {
                return Some(role);
            }
        }
    }
    None
}

fn states_to_i32_pair(states: &[State]) -> [i32; 2] {
    let mut bits: u64 = 0;
    for s in states {
        let ss = StateSet::new(*s);
        bits |= ss.bits();
    }
    // On the wire the state set is represented as two u32 words (low, high)
    // which get passed as i32 values via MatchArgs.
    [bits as u32 as i32, (bits >> 32) as u32 as i32]
}

// -----------------------------------------------------------------------------
// AT-SPI backend
// -----------------------------------------------------------------------------

pub struct AtspiBackend<'a> {
    config: &'a Config,
    window_system: &'a dyn WindowSystem,
    runtime: Runtime,
    toolkit: String,
    toolkit_version: String,
    scale_factor: f64,
}

impl<'a> AtspiBackend<'a> {
    pub fn new(config: &'a Config, window_system: &'a dyn WindowSystem) -> Self {
        let runtime = Runtime::new().expect("failed to create tokio runtime for atspi backend");
        Self {
            config,
            window_system,
            runtime,
            toolkit: String::new(),
            toolkit_version: String::new(),
            scale_factor: 1.0,
        }
    }

    fn app_rule(&self) -> Option<&ApplicationRule> {
        self.config
            .backends
            .atspi
            .application_rules
            .get(self.window_system.focused_application_name())
    }

    fn get_app_rules_for_atspi(&self, focused_app: &str) -> ApplicationRules {
        let rule = self.config.backends.atspi.application_rules.get(focused_app);
        ApplicationRules {
            scale_factor: rule.map(|r| r.scale_factor).unwrap_or(1.0),
            ..ApplicationRules::default()
        }
    }
}

impl<'a> Backend for AtspiBackend<'a> {
    fn get_children(&mut self) -> Result<Vec<Child>, AccessibleChildrenNotFound> {
        let app_name = self.window_system.focused_application_name().to_string();
        let window_extents = self.window_system.focused_window_extents();
        let ws_type = self.window_system.window_system_type();
        let focused_pid = self.window_system.focused_window_pid();

        // Snapshot the application rule so the async closure is `'static`.
        let rule = self.app_rule().cloned().unwrap_or_default();
        self.scale_factor = rule.scale_factor;

        let children = self
            .runtime
            .block_on(async move {
                collect_children_async(focused_pid, window_extents, ws_type, &rule).await
            })
            .map_err(|e| AccessibleChildrenNotFound {
                message: format!("atspi backend failed for {}: {}", app_name, e),
            })?;

        if children.is_empty() {
            return Err(AccessibleChildrenNotFound {
                message: format!("No accessible children found for {}", app_name),
            });
        }

        log::debug!(
            "Finished gathering hints for '{}'. Toolkit: {} v{}",
            app_name,
            self.toolkit,
            self.toolkit_version
        );

        Ok(children)
    }

    fn get_application_rules(&self) -> ApplicationRules {
        self.get_app_rules_for_atspi(self.window_system.focused_application_name())
    }
}

/// Async workhorse for [`AtspiBackend::get_children`]. All of the real
/// accessibility-bus traffic lives here; the sync wrapper just blocks on the
/// runtime.
async fn collect_children_async(
    focused_pid: u32,
    window_extents: (i32, i32, i32, i32),
    ws_type: WindowSystemType,
    rule: &ApplicationRule,
) -> Result<Vec<Child>, String> {
    let connection = AccessibilityConnection::open()
        .await
        .map_err(|e| format!("AccessibilityConnection::open failed: {}", e))?;

    let zbus_conn = connection.connection().clone();

    // Walk the desktop looking for a top-level window owned by `focused_pid`
    // that is currently ACTIVE.
    let desktop = AccessibleProxy::builder(&zbus_conn)
        .destination("org.a11y.atspi.Registry")
        .map_err(|e| e.to_string())?
        .path("/org/a11y/atspi/accessible/root")
        .map_err(|e| e.to_string())?
        .build()
        .await
        .map_err(|e| format!("failed to build registry root proxy: {}", e))?;

    let focused_window = find_focused_window(&zbus_conn, &desktop, focused_pid).await?;
    let Some(window_proxy) = focused_window else {
        return Err("no focused accessible window matched the active pid".into());
    };

    // Toolkit / version via the Application interface.
    let (toolkit, toolkit_version) = match window_proxy.get_application().await {
        Ok(app) => {
            let app_proxy = ApplicationProxy::builder(&zbus_conn)
                .destination(app.name.clone())
                .map_err(|e| e.to_string())?
                .path(app.path.clone())
                .map_err(|e| e.to_string())?
                .build()
                .await
                .map_err(|e| format!("failed to build application proxy: {}", e))?;
            (
                app_proxy.toolkit_name().await.unwrap_or_default(),
                app_proxy.version().await.unwrap_or_default(),
            )
        }
        Err(e) => {
            log::debug!("could not fetch application proxy: {}", e);
            (String::new(), String::new())
        }
    };

    let ctx = AtspiContext {
        window_extents,
        ws_type,
        toolkit,
        toolkit_version,
        scale_factor: rule.scale_factor,
        states: rule
            .states
            .iter()
            .filter_map(|s| state_from_config_str(s))
            .collect(),
        states_match_type: match_type_from_str(&rule.states_match_type),
        attributes: rule.attributes.clone(),
        attributes_match_type: match_type_from_str(&rule.attributes_match_type),
        roles: rule
            .roles
            .iter()
            .filter_map(|s| role_from_config_str(s))
            .collect(),
        roles_match_type: match_type_from_str(&rule.roles_match_type),
    };

    let mut children: Vec<Child> = Vec::new();
    gather_children(&zbus_conn, &window_proxy, &ctx, &mut children).await?;

    Ok(children)
}

struct AtspiContext {
    window_extents: (i32, i32, i32, i32),
    ws_type: WindowSystemType,
    toolkit: String,
    toolkit_version: String,
    scale_factor: f64,
    states: Vec<State>,
    states_match_type: MatchType,
    attributes: HashMap<String, String>,
    attributes_match_type: MatchType,
    roles: Vec<Role>,
    roles_match_type: MatchType,
}

/// Walk the desktop: `desktop -> application -> window`. Returns the first
/// window that is `ACTIVE` and whose process id matches `focused_pid`.
async fn find_focused_window<'a>(
    conn: &'a zbus::Connection,
    desktop: &'a AccessibleProxy<'_>,
    focused_pid: u32,
) -> Result<Option<AccessibleProxy<'a>>, String> {
    let apps = desktop
        .get_children()
        .await
        .map_err(|e| format!("desktop.get_children failed: {}", e))?;

    for app in apps {
        let app_proxy = match AccessibleProxy::builder(conn)
            .destination(app.name.clone())
            .and_then(|b| b.path(app.path.clone()))
        {
            Ok(b) => match b.build().await {
                Ok(p) => p,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        // Skip Gnome's mutter-x11-frames helper.
        if let Ok(desc) = app_proxy.description().await {
            if desc.contains("mutter-x11-frames") {
                continue;
            }
        }

        let windows = match app_proxy.get_children().await {
            Ok(ws) => ws,
            Err(_) => continue,
        };

        for win in windows {
            let win_proxy = match AccessibleProxy::builder(conn)
                .destination(win.name.clone())
                .and_then(|b| b.path(win.path.clone()))
            {
                Ok(b) => match b.build().await {
                    Ok(p) => p,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };

            let Ok(state) = win_proxy.get_state().await else {
                continue;
            };
            if !state.contains(State::Active) {
                continue;
            }

            if focused_pid != 0 {
                if let Ok(app) = win_proxy.get_application().await {
                    if let Ok(app_only_builder) = ApplicationProxy::builder(conn)
                        .destination(app.name)
                        .and_then(|b| b.path(app.path))
                    {
                        if let Ok(app_only) = app_only_builder.build().await {
                            let pid = app_only.id().await.unwrap_or(0);
                            if pid != 0 && pid as u32 != focused_pid {
                                continue;
                            }
                        }
                    }
                }
            }

            return Ok(Some(win_proxy));
        }
    }
    Ok(None)
}

/// Port of `AtspiBackend.get_children_of_interest` — tries the Collection
/// interface first, falls back to a recursive walk.
async fn gather_children(
    conn: &zbus::Connection,
    root: &AccessibleProxy<'_>,
    ctx: &AtspiContext,
    children: &mut Vec<Child>,
) -> Result<(), String> {
    let collection_builder = CollectionProxy::builder(conn)
        .destination(root.destination().to_owned())
        .and_then(|b| b.path(root.path().to_owned()));

    if let Ok(builder) = collection_builder {
        if let Ok(coll) = builder.build().await {
            let state_bits = states_to_i32_pair(&ctx.states);
            let role_codes: Vec<i32> = ctx.roles.iter().map(|r| *r as i32).collect();
            let attrs_ref: HashMap<&str, &str> = ctx
                .attributes
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let interfaces: &[&str] = &[];
            let rule_args: atspi::MatchArgs = (
                &state_bits,
                ctx.states_match_type,
                attrs_ref,
                ctx.attributes_match_type,
                &role_codes,
                ctx.roles_match_type,
                interfaces,
                MatchType::All,
                false,
            );

            match coll
                .get_matches(&rule_args, SortOrder::Canonical, 0, true)
                .await
            {
                Ok(matches) => {
                    for m in matches {
                        let proxy = match AccessibleProxy::builder(conn)
                            .destination(m.name)
                            .and_then(|b| b.path(m.path))
                        {
                            Ok(b) => match b.build().await {
                                Ok(p) => p,
                                Err(_) => continue,
                            },
                            Err(_) => continue,
                        };

                        if let Some(child) = accessible_to_child(conn, &proxy, ctx).await {
                            children.push(child);
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    log::debug!(
                        "Collection.get_matches failed ({}); falling back to recursion.",
                        e
                    );
                }
            }
        }
    }

    // Fallback: recursive walk with per-element validation.
    recursive_gather(conn, root, ctx, children).await
}

/// Convert an AT-SPI Accessible proxy to a [`Child`] using the same coordinate
/// rules as the Python reference implementation.
async fn accessible_to_child(
    conn: &zbus::Connection,
    proxy: &AccessibleProxy<'_>,
    ctx: &AtspiContext,
) -> Option<Child> {
    let component = ComponentProxy::builder(conn)
        .destination(proxy.destination().to_owned())
        .ok()?
        .path(proxy.path().to_owned())
        .ok()?
        .build()
        .await
        .ok()?;

    let (abs, rel, size) = relative_and_absolute_extents(&component, ctx).await?;

    if rel.0 < 0.0 || rel.1 < 0.0 {
        return None;
    }

    Some(Child {
        absolute_position: abs,
        relative_position: rel,
        width: size.0,
        height: size.1,
    })
}

/// Port of `get_relative_and_absolute_extents`.
async fn relative_and_absolute_extents(
    component: &ComponentProxy<'_>,
    ctx: &AtspiContext,
) -> Option<((f64, f64), (f64, f64), (f64, f64))> {
    let (start_x, start_y, _, _) = ctx.window_extents;
    let scale = ctx.scale_factor;

    // GTK4 toolkit detection — mirrors the Python branch.
    let gtk4 = ctx.toolkit.eq_ignore_ascii_case("GTK")
        && ctx
            .toolkit_version
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .is_some_and(|m| m >= 4);

    if matches!(ctx.ws_type, WindowSystemType::Wayland) || gtk4 {
        let (mut x, y, w, h) = component.get_extents(CoordType::Window).await.ok()?;
        if x == -1 {
            x = x.abs();
        }
        let x = x as f64 * scale;
        let y = y as f64 * scale;
        let w = w as f64 * scale;
        let h = h as f64 * scale;
        return Some((
            (x + start_x as f64, y + start_y as f64),
            (x, y),
            (w, h),
        ));
    }

    let (x, y, w, h) = component.get_extents(CoordType::Screen).await.ok()?;
    let x = x as f64 * scale;
    let y = y as f64 * scale;
    Some((
        (x, y),
        (x - start_x as f64, y - start_y as f64),
        (w as f64 * scale, h as f64 * scale),
    ))
}

/// Port of `validate_match_conditions` for both state and role.
async fn validate_match(proxy: &AccessibleProxy<'_>, ctx: &AtspiContext) -> bool {
    let state_ok = match ctx.states_match_type {
        MatchType::All | MatchType::Empty => match proxy.get_state().await {
            Ok(state_set) => ctx.states.iter().all(|s| state_set.contains(*s)),
            Err(_) => false,
        },
        MatchType::Any => match proxy.get_state().await {
            Ok(state_set) => ctx.states.iter().any(|s| state_set.contains(*s)),
            Err(_) => false,
        },
        MatchType::NA => match proxy.get_state().await {
            Ok(state_set) => !ctx.states.iter().any(|s| state_set.contains(*s)),
            Err(_) => true,
        },
        MatchType::Invalid => true,
    };
    if !state_ok {
        return false;
    }

    match ctx.roles_match_type {
        MatchType::All | MatchType::Empty | MatchType::Any => match proxy.get_role().await {
            Ok(role) => ctx.roles.contains(&role),
            Err(_) => false,
        },
        MatchType::NA => match proxy.get_role().await {
            Ok(role) => !ctx.roles.contains(&role),
            Err(_) => true,
        },
        MatchType::Invalid => true,
    }
}

/// Non-recursive walk (explicit stack) to avoid async recursion issues.
async fn recursive_gather(
    conn: &zbus::Connection,
    root: &AccessibleProxy<'_>,
    ctx: &AtspiContext,
    children: &mut Vec<Child>,
) -> Result<(), String> {
    let mut stack: Vec<(String, String)> =
        vec![(root.destination().to_string(), root.path().to_string())];

    while let Some((dest, path)) = stack.pop() {
        let proxy = match AccessibleProxy::builder(conn)
            .destination(dest.clone())
            .and_then(|b| b.path(path.clone()))
        {
            Ok(b) => match b.build().await {
                Ok(p) => p,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        if validate_match(&proxy, ctx).await {
            if let Some(child) = accessible_to_child(conn, &proxy, ctx).await {
                children.push(child);
            }
        }

        if let Ok(kids) = proxy.get_children().await {
            for k in kids {
                stack.push((k.name, k.path.to_string()));
            }
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// OpenCV backend
// -----------------------------------------------------------------------------

pub struct OpenCVBackend<'a> {
    config: &'a Config,
    window_system: &'a dyn WindowSystem,
}

impl<'a> OpenCVBackend<'a> {
    pub fn new(config: &'a Config, window_system: &'a dyn WindowSystem) -> Self {
        Self {
            config,
            window_system,
        }
    }

    fn app_rule(&self) -> Option<&OpenCvApplicationRule> {
        self.config
            .backends
            .opencv
            .application_rules
            .get(self.window_system.focused_application_name())
    }

    fn get_app_rules_for_opencv(&self, focused_app: &str) -> ApplicationRules {
        let rule = self.config.backends.opencv.application_rules.get(focused_app);
        ApplicationRules {
            scale_factor: 1.0,
            opencv_kernel_size: rule.map(|r| r.kernel_size).unwrap_or(6),
            opencv_canny_min_val: rule.map(|r| r.canny_min_val).unwrap_or(100),
            opencv_canny_max_val: rule.map(|r| r.canny_max_val).unwrap_or(200),
        }
    }
}

impl<'a> Backend for OpenCVBackend<'a> {
    fn get_children(&mut self) -> Result<Vec<Child>, AccessibleChildrenNotFound> {
        let (win_x, win_y, win_w, win_h) = self.window_system.focused_window_extents();
        let app_name = self.window_system.focused_application_name().to_string();

        if win_w <= 0 || win_h <= 0 {
            return Err(AccessibleChildrenNotFound {
                message: format!("Invalid window extents for {}", app_name),
            });
        }

        let rule = self.app_rule().cloned().unwrap_or_default();

        // TODO: sway bar_height offset when WindowSystem trait exposes it.
        let screen = screenshots::Screen::from_point(win_x, win_y).map_err(|e| {
            AccessibleChildrenNotFound {
                message: format!("Failed to locate screen for {}: {}", app_name, e),
            }
        })?;

        let rgba = screen
            .capture_area(win_x, win_y, win_w as u32, win_h as u32)
            .map_err(|e| AccessibleChildrenNotFound {
                message: format!("Failed to capture screenshot for {}: {}", app_name, e),
            })?;

        // `screenshots` returns RGBA; convert to grayscale via `image`.
        let (w, h) = (rgba.width(), rgba.height());
        let rgba_img = image::RgbaImage::from_raw(w, h, rgba.into_raw()).ok_or_else(|| {
            AccessibleChildrenNotFound {
                message: format!("Malformed screenshot buffer for {}", app_name),
            }
        })?;
        let gray: GrayImage = image::DynamicImage::ImageRgba8(rgba_img).to_luma8();

        let edges = canny(&gray, rule.canny_min_val as f32, rule.canny_max_val as f32);

        // Python uses `ones((kernel_size, kernel_size))` — a square
        // structuring element. LInf dilation with radius `kernel_size/2`
        // approximates the same effect.
        let kernel_radius = (rule.kernel_size / 2).max(1) as u8;
        let dilated = dilate(&edges, Norm::LInf, kernel_radius);

        let contours = find_contours::<i32>(&dilated);
        let mut children = Vec::with_capacity(contours.len());

        for contour in &contours {
            if contour.points.is_empty() {
                continue;
            }
            let mut min_x = i32::MAX;
            let mut min_y = i32::MAX;
            let mut max_x = i32::MIN;
            let mut max_y = i32::MIN;
            for p in &contour.points {
                if p.x < min_x {
                    min_x = p.x;
                }
                if p.y < min_y {
                    min_y = p.y;
                }
                if p.x > max_x {
                    max_x = p.x;
                }
                if p.y > max_y {
                    max_y = p.y;
                }
            }
            let width = (max_x - min_x + 1) as f64;
            let height = (max_y - min_y + 1) as f64;
            children.push(Child {
                absolute_position: ((min_x + win_x) as f64, (min_y + win_y) as f64),
                relative_position: (min_x as f64, min_y as f64),
                width,
                height,
            });
        }

        log::debug!("Finished gathering opencv hints for '{}'", app_name);

        if children.is_empty() {
            return Err(AccessibleChildrenNotFound {
                message: format!("No children found for {}", app_name),
            });
        }

        Ok(children)
    }

    fn get_application_rules(&self) -> ApplicationRules {
        self.get_app_rules_for_opencv(self.window_system.focused_application_name())
    }
}

// -----------------------------------------------------------------------------
// Factory consumed by `hints/src/main.rs`.
// -----------------------------------------------------------------------------

pub fn get_backends<'a>(
    config: &'a Config,
    window_system: &'a dyn WindowSystem,
) -> Vec<(BackendType, Box<dyn Backend + 'a>)> {
    let mut backends = Vec::new();

    for backend_name in &config.backends.enable {
        match backend_name.as_str() {
            "atspi" => {
                backends.push((
                    BackendType::Atspi,
                    Box::new(AtspiBackend::new(config, window_system)) as Box<dyn Backend + '_>,
                ));
            }
            "opencv" => {
                backends.push((
                    BackendType::OpenCV,
                    Box::new(OpenCVBackend::new(config, window_system)) as Box<dyn Backend + '_>,
                ));
            }
            "yolo" => match yolo::YoloBackend::new(config, window_system) {
                Ok(b) => backends.push((BackendType::Yolo, Box::new(b) as Box<dyn Backend + '_>)),
                Err(e) => log::warn!("yolo backend disabled: {}", e),
            },
            _ => {}
        }
    }

    backends
}
