//! `sandbox_engine/mounts.rs`
//!
//! Handles all filesystem mounting logic (bind-mounts) for the sandbox.
//! This is the most critical part of the sandbox's security and compatibility.

use std::env;
use std::process::Command;

/// Mounts static hardcoded system directories needed to run typical binaries.
///
/// This ensures that the sandboxed process has access to shared libraries (`/lib`),
/// standard tools (`/bin`), and core system configuration (`/etc`).
pub fn apply_system_mounts(bwrap: &mut Command, gui: bool) {
    let standard_paths = [
        "/usr",
        "/bin",
        "/lib",
        "/lib64",
        "/etc/alternatives",
        "/snap",
        "/opt",
    ];

    for path in standard_paths {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--ro-bind", path, path]);
        }
    }

    // Ensure $HOME exists inside the sandbox as an empty directory in the tmpfs.
    // Without this, tools like npm, pip, and cargo cannot access ~/.npm, ~/.cache,
    // ~/.cargo etc. and fail immediately — even though HOME is set in the environment.
    // We do NOT bind-mount the real home dir (no host files leak in); the directory
    // is created empty in the synthetic root and any writes are discarded on exit.
    if let Ok(home) = std::env::var("HOME") {
        bwrap.arg("--dir").arg(&home);
    }

    // Snap runtime support: when node/python/etc. are installed as snap packages,
    // their launcher binary (/snap/bin/X → /usr/bin/snap) connects to the host
    // snapd daemon via Unix sockets.  Without these bind-mounts, snap hangs
    // indefinitely waiting for a socket that doesn't exist in the sandbox.
    for path in &["/run/snapd.socket", "/run/snapd-snap.socket"] {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--bind", path, path]);
        }
    }
    // Snap namespace directory (used by snapd for snap confinement bookkeeping)
    if std::path::Path::new("/run/snapd/ns").exists() {
        bwrap.args(["--bind", "/run/snapd/ns", "/run/snapd/ns"]);
    }
    // Per-user snapd session agent socket (path derived from XDG_RUNTIME_DIR
    // which is already forwarded into the sandbox via apply_environment)
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let agent_sock = format!("{}/snapd-session-agent.socket", runtime_dir);
        if std::path::Path::new(&agent_sock).exists() {
            bwrap.arg("--dir").arg(&runtime_dir);
            bwrap.args(["--bind", &agent_sock, &agent_sock]);
        }
    }

    if gui {
        // --- GUI and Hardware Support ---
        // These mounts are only active when the --gui flag is used.
        
        // /usr/share/fonts: read-only is correct, fonts are static data.
        if std::path::Path::new("/usr/share/fonts").exists() {
            bwrap.args(["--ro-bind", "/usr/share/fonts", "/usr/share/fonts"]);
        }

        // /tmp/.X11-unix: X11 display sockets live here.
        // Must be --bind (read-write) — X11 clients connect by *writing* to
        // these Unix sockets.  Using --ro-bind here silently breaks all X11 GUI apps.
        if std::path::Path::new("/tmp/.X11-unix").exists() {
            bwrap.args(["--bind", "/tmp/.X11-unix", "/tmp/.X11-unix"]);
        }

        // X11 Authorization: pass .Xauthority so binary apps can connect
        if let Ok(xauth) = std::env::var("XAUTHORITY") {
            if std::path::Path::new(&xauth).exists() {
                bwrap.args(["--ro-bind", &xauth, &xauth]);
            }
        }
        // Fallback to default path
        if let Ok(home) = std::env::var("HOME") {
            let xauth_path = format!("{}/.Xauthority", home);
            if std::path::Path::new(&xauth_path).exists() {
                bwrap.args(["--ro-bind", &xauth_path, &xauth_path]);
            }
        }

        // GPU Acceleration: /dev/dri is required for hardware-accelerated rendering
        if std::path::Path::new("/dev/dri").exists() {
            bwrap.args(["--dev-bind", "/dev/dri", "/dev/dri"]);
        }

        // System information for hardware discovery (MESA needs this)
        if std::path::Path::new("/sys").exists() {
            bwrap.args(["--ro-bind", "/sys", "/sys"]);
        }

        // Shared memory (required for efficient GPU rendering buffer swaps)
        if std::path::Path::new("/dev/shm").exists() {
            bwrap.args(["--bind", "/dev/shm", "/dev/shm"]);
        }

        if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
            // Wayland socket
            if let Ok(wayland_display) = env::var("WAYLAND_DISPLAY") {
                let socket = format!("{}/{}", runtime_dir, wayland_display);
                if std::path::Path::new(&socket).exists() {
                    bwrap.args(["--bind", &socket, &socket]);
                }
            }

            // D-Bus User session socket
            let dbus_socket = format!("{}/bus", runtime_dir);
            if std::path::Path::new(&dbus_socket).exists() {
                bwrap.args(["--bind", &dbus_socket, &dbus_socket]);
            }

            // dconf profile / settings (required for GTK4/GNOME)
            let dconf_dir = format!("{}/dconf", runtime_dir);
            if std::path::Path::new(&dconf_dir).exists() {
                bwrap.args(["--bind", &dconf_dir, &dconf_dir]);
            }

            // Accessibility bus
            let at_spi_dir = format!("{}/at-spi", runtime_dir);
            if std::path::Path::new(&at_spi_dir).exists() {
                bwrap.args(["--bind", &at_spi_dir, &at_spi_dir]);
            }
        }
    }
}
