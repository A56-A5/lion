use std::env;
use std::process::Command;

/// Mounts static hardcoded system directories needed to run typical binaries.
pub fn apply_system_mounts(bwrap: &mut Command, gui: bool) {
    let standard_paths = ["/usr", "/bin", "/lib", "/lib64", "/etc/alternatives"];

    for path in standard_paths {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--ro-bind", path, path]);
        }
    }

    if gui {
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

        if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR")
            && let Ok(wayland_display) = env::var("WAYLAND_DISPLAY")
        {
            let socket = format!("{}/{}", runtime_dir, wayland_display);
            if std::path::Path::new(&socket).exists() {
                bwrap.args(["--bind", &socket, &socket]);
            }
        }
    }
}
