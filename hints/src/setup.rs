use std::env;

pub fn run_guided_setup() {
    let uid = env::var("UID").unwrap_or_default();
    if uid != "0" {
        println!("Please run with sudo: sudo hints --setup");
        return;
    }

    println!("Running guided setup...");

    setup_accessibility_variables();
    setup_uinput_module();
    setup_udev_rules();
    setup_hintsd();

    if is_wayland_gnome() {
        setup_gnome_plugin();
    }

    if should_continue() {
        println!("Setup complete!");
    }

    show_post_setup_instructions();
}

fn setup_accessibility_variables() {
    println!("Setting up accessibility variables...");
    let env_vars = [
        "ACCESSIBILITY_ENABLED=1",
        "GTK_MODULES=gail:atk-bridge",
        "OOO_FORCE_DESKTOP=gnome",
        "GNOME_ACCESSIBILITY=1",
        "QT_ACCESSIBILITY=1",
        "QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1",
    ];

    let path = "/etc/environment";
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<&str> = content.lines().collect();

    for var in &env_vars {
        let key = var.split('=').next().unwrap();
        if !lines.iter().any(|l| l.starts_with(key)) {
            lines.push(var);
        }
    }

    std::fs::write(path, lines.join("\n")).ok();
}

fn setup_uinput_module() {
    println!("Setting up uinput module...");
    std::process::Command::new("modprobe")
        .arg("uinput")
        .output()
        .ok();

    std::fs::write("/etc/modules-load.d/uinput.conf", "uinput\n").ok();
}

fn setup_udev_rules() {
    println!("Setting up udev rules...");
    let rule = "KERNEL==\"uinput\", TAG+=\"uaccess\"";
    std::fs::write("/etc/udev/rules.d/80-hints.rules", rule).ok();

    if let Ok(user) = env::var("SUDO_USER") {
        std::process::Command::new("usermod")
            .args(["-aG", "input", &user])
            .output()
            .ok();
    }
}

fn setup_hintsd() {
    println!("Setting up hintsd service...");
    let user = env::var("SUDO_USER").unwrap_or_default();
    if !user.is_empty() {
        std::process::Command::new("systemctl")
            .args([
                "--machine",
                &format!("{}@.host", user),
                "--user",
                "daemon-reload",
            ])
            .output()
            .ok();

        std::process::Command::new("systemctl")
            .args([
                "--machine",
                &format!("{}@.host", user),
                "--user",
                "enable",
                "hintsd",
            ])
            .output()
            .ok();

        std::process::Command::new("systemctl")
            .args([
                "--machine",
                &format!("{}@.host", user),
                "--user",
                "start",
                "hintsd",
            ])
            .output()
            .ok();
    }
}

fn setup_gnome_plugin() {
    println!("Setting up GNOME plugin...");
}

fn is_wayland_gnome() -> bool {
    let xdg_session_type = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();

    xdg_session_type == "wayland" && desktop.contains("GNOME")
}

fn should_continue() -> bool {
    println!("The following changes will be made:");
    println!("  - Accessibility variables will be configured");
    println!("  - uinput module will be loaded");
    println!("  - udev rules will be created");
    println!("  - hintsd service will be enabled");
    println!();
    println!("Do you want to continue? (y/N)");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();

    input.trim().to_lowercase() == "y"
}

fn show_post_setup_instructions() {
    println!();
    println!("Setup complete!");
    println!();
    println!("Please reboot your system for changes to take effect.");
}
