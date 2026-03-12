# L.I.O.N Sandbox Exposure Analysis

By default, L.I.O.N follows a **"Deny All"** philosophy. It builds a synthetic root from scratch and whitelists only the absolute minimum required for a standard process to function.

This document details exactly what is exposed, hidden, and monitored when running inside L.I.O.N.

---

## 1. Core Isolation (The Wall)

L.I.O.N unshares all major Linux namespaces on every run:

| Namespace | Flag | Effect |
| :--- | :--- | :--- |
| User | `--unshare-user` | UID/GID remapped. Cannot gain host root. |
| PID | `--unshare-pid` | Isolated process tree. Cannot see host PIDs. |
| Network | `--unshare-net` | Zero interfaces by default. No internet, no LAN, no localhost. |
| IPC | `--unshare-ipc` | No shared memory or message queues with host processes. |
| UTS | `--unshare-uts` | Fake hostname: reports `lion`, not your machine name. |
| Cgroup | `--unshare-cgroup` | Isolated cgroup tree. |

Additional hardening flags active on every run:

- **`--die-with-parent`** — if the `lion` process is killed or crashes, the sandbox is killed instantly. No orphan processes ever remain.
- **`--new-session`** — detaches the sandbox from your terminal session. It cannot send signals (Ctrl+C) to your host shell.
- **`--tmpfs /`** — the root filesystem starts completely empty. Nothing from the host is visible unless explicitly bind-mounted.

---

## 2. Filesystem Root Construction

The sandbox root is built up from scratch in a defined order:

1. `--tmpfs /` — blank root
2. `--dir` stubs created: `/usr`, `/bin`, `/lib`, `/lib64`, `/etc`, `/run`
3. System paths bind-mounted read-only (see table below)
4. Project directory bind-mounted (read-write or read-only per config)
5. Optional module paths added (GPU, Wayland, audio — only when explicitly enabled)

| Path | Mount type | Purpose | Risk |
| :--- | :--- | :--- | :--- |
| `/usr` | ro-bind | Binaries and libraries | Low |
| `/bin` | ro-bind | Essential commands | Low |
| `/lib`, `/lib64` | ro-bind | C library, dynamic linker | Low |
| `/etc/alternatives` | ro-bind | Version symlinks | Low |
| `/snap` | ro-bind | Ubuntu Snap tools (if present) | Medium — exposes installed snap names |

---

## 3. Project Exposure (The Workspace)

- **Project root**: the directory where `lion run` is invoked is mounted **read-write** by default, allowing normal build output, test artifacts, and log files.
- **`src/` protection**: when the project root is read-write, `src/` is **re-mounted read-only** on top. This prevents a malicious package from overwriting your source code mid-run.
- **Explicit read-only paths**: additional paths passed via `--ro /path` are mounted read-only inside the sandbox.

---

## 4. Environment Variables (The Context)

**All host environment variables are wiped first** via `--clearenv`. This means `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, `NPM_TOKEN`, `SSH_AUTH_SOCK`, `DATABASE_URL`, and every other credential or shell alias in your environment are **completely invisible** inside the sandbox.

Only the following are re-added explicitly:

| Variable | Purpose |
| :--- | :--- |
| `HOME`, `USER`, `LOGNAME` | Basic user identity |
| `PATH` | Binary discovery |
| `LANG`, `LC_ALL` | Text encoding / locale |
| `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` | Standard XDG paths |
| `XAUTHORITY` | **Only with `--gui`** — X11 display authentication |
| `DISPLAY`, `WAYLAND_DISPLAY` | **Only with `--gui`** — display server socket names |

---

## 5. Optional Feature Exposures (The Holes)

### `--net=none` (default)
Network namespace is fully unshared. Zero interfaces. Outbound connections are impossible at the kernel level.

### `--net=dns`
Shares the host network namespace. Only `/etc/resolv.conf` is bind-mounted. DNS resolution works; nothing else is explicitly provided.

### `--net=full`
Shares the host network namespace completely. Also mounts `/etc/resolv.conf`, `/etc/ssl`, and `/etc/pki` read-only so HTTPS works. Full internet access.

### `--gui`
The widest expansion of the sandbox surface — required for graphical applications:

| What | Why | Risk |
| :--- | :--- | :--- |
| `/tmp/.X11-unix` (bind rw) | X11 display sockets | Medium — can observe X11 events |
| `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY` (bind rw) | Wayland compositor socket | Medium |
| `$XAUTHORITY` (ro-bind) | X11 auth cookie | Low if app is trusted |
| `/dev/dri` (dev-bind) | GPU hardware rendering | Medium — raw device access |
| `/sys` (ro-bind) | Hardware topology for MESA/GPU | Low |
| `/dev/shm` (bind rw) | Shared memory for GPU buffer swaps | Medium — shared with host |
| `$XDG_RUNTIME_DIR/bus` | D-Bus user session | High — can talk to host services |
| `$XDG_RUNTIME_DIR/at-spi` | Accessibility bus | Low |

---

## 6. Live Access Monitor

L.I.O.N runs specialized background monitor logic:

**TUI Separation**:
- Launches a separate terminal (`gnome-terminal` or `kitty`) for events.
- Communications happen via a temporary FIFO in `/tmp/lion-monitor-<pid>`.
- Gracefully falls back to inline monitoring if no separate terminal is available.

**inotify watcher** (allowed access tracking):
- Watches all bind-mounted paths for `ACCESS`, `OPEN`, `MODIFY`, `CREATE`, `DELETE`.
- Reports every file the sandboxed process actually touches.

**stderr parser** (blocked access tracking):
- Reads bwrap's stderr pipe.
- Reports attempts that the sandbox denied (e.g., `Permission denied` on `/etc/shadow`).

Both streams are printed live with timestamps, ANSI color, and event type tags.

---

## 7. What Remains Hidden (The Secrets)

Even with every flag enabled, L.I.O.N **never** exposes:

- **`~/.ssh/`** — private keys, `known_hosts`, SSH config
- **`~/.gnupg/`** — GPG keyring
- **`~/Documents/`, `~/Downloads/`, `~/Desktop/`** — personal files
- **Browser profiles** — passwords, cookies, session tokens
- **`/etc/shadow`**, **`/etc/sudoers`** — system credentials
- **`/var/log/`**, **`/root/`** — system logs, root home
- **`/media/`**, **`/mnt/`** — mounted drives and external storage
- **Any credential in environment variables** — wiped by `--clearenv` before the sandbox starts
