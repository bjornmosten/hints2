use utils::{AccessibleChildrenNotFound, Child, Config};
use window_systems::WindowSystem;

pub trait Backend {
    fn get_children(&mut self) -> Result<Vec<Child>, AccessibleChildrenNotFound>;
    fn get_application_rules(&self) -> ApplicationRules;
}

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

pub struct AtspiBackend<'a> {
    config: &'a Config,
    window_system: &'a dyn WindowSystem,
}

impl<'a> AtspiBackend<'a> {
    pub fn new(config: &'a Config, window_system: &'a dyn WindowSystem) -> Self {
        Self {
            config,
            window_system,
        }
    }

    fn get_app_rules_for_atspi(&self, focused_app: &str) -> ApplicationRules {
        let backend_config = &self.config.backends.atspi;
        let app_rules = backend_config.application_rules.get(focused_app);

        ApplicationRules {
            scale_factor: app_rules.map(|r| r.scale_factor).unwrap_or(1.0),
            opencv_kernel_size: 6,
            opencv_canny_min_val: 100,
            opencv_canny_max_val: 200,
        }
    }
}

impl<'a> Backend for AtspiBackend<'a> {
    fn get_children(&mut self) -> Result<Vec<Child>, AccessibleChildrenNotFound> {
        Ok(Vec::new())
    }

    fn get_application_rules(&self) -> ApplicationRules {
        self.get_app_rules_for_atspi(&self.window_system.focused_application_name())
    }
}

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

    fn get_app_rules_for_opencv(&self, focused_app: &str) -> ApplicationRules {
        let backend_config = &self.config.backends.opencv;
        let app_rules = backend_config.application_rules.get(focused_app);

        ApplicationRules {
            scale_factor: 1.0,
            opencv_kernel_size: app_rules.map(|r| r.kernel_size).unwrap_or(6),
            opencv_canny_min_val: app_rules.map(|r| r.canny_min_val).unwrap_or(100),
            opencv_canny_max_val: app_rules.map(|r| r.canny_max_val).unwrap_or(200),
        }
    }
}

impl<'a> Backend for OpenCVBackend<'a> {
    fn get_children(&mut self) -> Result<Vec<Child>, AccessibleChildrenNotFound> {
        let extents = self.window_system.focused_window_extents();
        let app_name = self.window_system.focused_application_name();

        let screen = screenshots::Screen::from_point(extents.0, extents.1);

        if screen.is_err() {
            return Err(AccessibleChildrenNotFound {
                message: format!("Failed to capture screenshot for {}", app_name),
            });
        }

        let img =
            screen
                .unwrap()
                .capture_area(extents.0, extents.1, extents.2 as u32, extents.3 as u32);

        if img.is_err() {
            return Err(AccessibleChildrenNotFound {
                message: format!("Failed to capture screenshot for {}", app_name),
            });
        }

        let _raw = img.unwrap().into_raw();
        let mut children = Vec::new();

        let (win_x, win_y, _, _) = extents;

        children.push(Child {
            absolute_position: (win_x as f64, win_y as f64),
            relative_position: (0.0, 0.0),
            width: 100.0,
            height: 100.0,
        });

        if children.is_empty() {
            return Err(AccessibleChildrenNotFound {
                message: format!("No children found for {}", app_name),
            });
        }

        Ok(children)
    }

    fn get_application_rules(&self) -> ApplicationRules {
        self.get_app_rules_for_opencv(&self.window_system.focused_application_name())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BackendType {
    Atspi,
    OpenCV,
}

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
            _ => {}
        }
    }

    backends
}
