# L.I.O.N Sandbox Exposure Analysis

By default, L.I.O.N follows a **"Deny All"** philosophy. It creates a blank Linux namespace and whitelist-binds only the absolute minimum required for a standard process to function.

This document details exactly what is "leaked" or intentionally exposed to applications running inside the sandbox.

---

## 1. Core Isolation (The Wall)

L.I.O.N uses Linux Namespaces to isolate the following by default:
- **PID Namespace**: The app cannot see any other processes on your system. It thinks it is PID 2.
- **User Namespace**: The app maps your UID (e.g., 1000) to a namespaced UID. It cannot gain root privileges on the host.
- **Network Namespace**: By default (without `--network`), there are **zero** network interfaces. No internet, no LAN, no localhost.
- **IPC Namespace**: The app cannot use shared memory or message queues to talk to host processes.
- **UTS Namespace**: The app sees a generic hostname, not your machine's real name.

---

## 2. Hardcoded System Exposures (The Foundation)

To allow binaries (like `ls`, `node`, `python`) to even start, we must expose system libraries. These are mounted **Read-Only**:

| Path | Purpose | Risk |
| :--- | :--- | :--- |
| `/usr` | Standard binaries and libraries. | Low. General system code. |
| `/bin` | Essential system commands. | Low. Standard tools. |
| `/lib`, `/lib64` | The C library and dynamic linker. | Low. Required for execution. |
| `/etc/alternatives` | Symlinks for default versioning. | Low. |
| `/snap` | Required for Ubuntu Snap-packaged tools. | Medium. Allows discovery of other installed snaps. |

---

## 3. Project Exposure (The Workspace)

L.I.O.N is a "per-execution" sandbox designed for developers.

- **The Project Root**: The directory where you run `lion` is mounted **Read-Write**. This allows you to compile, run scripts, and generate logs.
- **`src/` Protection**: As a safety feature, L.I.O.N **re-mounts the `src/` directory as Read-Only**. This prevents a buggy test or malicious NPM/Cargo package from overwriting your source code while it runs.

---

## 4. Environment Variables (The Context)

We strip almost all environment variables. Only these are passed through:
- `HOME`, `USER`, `LOGNAME`: Basic identity.
- `PATH`: To find binaries.
- `LANG`, `LC_ALL`: To maintain your keyboard/text encoding.
- `XDG_*`: Standard paths for config and cache.
- `XAUTHORITY`: **Only with `--gui`**. Required for display authentication.

---

## 5. Optional Feature Exposures (The Holes)

### Using `--network`
When enabled, the sandbox shares the host network namespace.
- **Exposed**: Your full internet connection, local network access, and localhost.
- **Security Info**: We bind `/etc/resolv.conf`, `/etc/ssl`, and `/etc/pki` read-only so DNS and HTTPS work correctly.

### Using `--gui`
This is the "widest" hole in the sandbox, required for visual apps.
- **X11/Wayland Sockets**: `/tmp/.X11-unix` and `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`.
- **GPU Hardware**: `/dev/dri` (Direct Rendering Infrastructure) and `/sys` (Hardware probing).
- **Communication**: `/run/user/1000/bus` (D-Bus) and `/run/user/1000/at-spi` (Accessibility).
- **Sensitive files**: `$XAUTHORITY` (X11 cookie) and `/dev/shm` (Shared memory).

---

## 6. What remains HIDDEN (The Secrets)

Even with all flags enabled, L.I.O.N **never** exposes:
- **Your Home Directory**: Apps cannot see your Documents, Downloads, SSH keys, or Browser profiles (unless they are inside the `lion` project folder).
- **Sensitive System Files**: No access to `/etc/shadow`, `/etc/sudoers`, `/var/log`, or `/root`.
- **Other Mounted Drives**: No access to `/media`, `/mnt`, or other internal partitions.
