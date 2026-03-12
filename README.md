# L.I.O.N

> Wrap any command in a hardened Linux sandbox. See every file it touches, every blocked attempt, in real time.

L.I.O.N is a per-execution filesystem sandbox for Linux using Bubblewrap (`bwrap`). Every `lion run` creates a **fresh, independent namespace cage** from scratch. When the program exits, the cage is gone.

## Features

- **Full namespace isolation** — PID, user, IPC, UTS, cgroup, and network namespaces, all unshared by default.
- **Synthetic root** — the sandbox starts with a blank `/`. Only whitelisted paths are bind-mounted in. Nothing leaks.
- **Complete environment wipe** — `--clearenv` runs first. `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, `NPM_TOKEN`, shell aliases — all invisible inside the cage.
- **No orphan processes** — `--die-with-parent` ensures the sandbox is killed the moment `lion` exits or crashes.
- **Fake identity** — sandbox reports hostname `lion`, detached from your real terminal session (`--new-session`).
- **Live access monitor** — inotify watchers on mounted paths report every file READ, WRITE, CREATE, and DELETE in real time in a separate terminal window. `bwrap` stderr is parsed simultaneously to catch blocked permission errors.
- **Real-time perf monitor** — CPU and RAM usage streamed as an ASCII bar graph with spark-line history, in a third terminal window.
- **Source protection** — `src/` is re-mounted read-only even when the project root is read-write.
- **Domain-filtered networking** — `--net=allow` runs a built-in Python proxy; only domains in `proxy.toml` (or the built-in defaults) can be reached. `--net=full` gives unrestricted access.
- **Selective capabilities** — opt-in to network (`--net`) or saved optional modules (`--optional`, `saved.toml`) only when needed.

For a full breakdown of what is exposed vs. hidden, see [EXPOSURES.md](EXPOSURES.md).

## System Setup

L.I.O.N requires `bwrap` on your system.
On Ubuntu 24+ and other distros that restrict unprivileged user namespaces via AppArmor, run the one-time setup:

```bash
# Creates a targeted AppArmor profile specifically for bwrap
sudo lion install
```

## Quick Start for New Users

If this is your first time using L.I.O.N, follow this order:

1. Install `bwrap` on your system.
1. Run `sudo lion install` once if your distro restricts user namespaces.
1. Move into the project you want to sandbox.
1. Start with a harmless command:

```bash
lion run -- pwd
lion run -- ls -la
```

1. If your command needs internet, try:

```bash
lion run --net=allow -- npm install
```

1. If your app needs GUI or desktop access, enable only the modules you need:

```bash
lion saved status
lion run --optional X11 -- xterm
```

## Core Idea

L.I.O.N works with three main layers:

1. **Filesystem isolation** — only mounted paths are visible.
2. **Network control** — choose `none`, `allow`, or `full`.
3. **Optional modules** — selectively expose things like X11, Wayland, GPU, fonts, or D-Bus.

The safest default is:

```bash
lion run -- your-command
```

Then add only what the program actually needs.

## Usage

```bash
# Basic sandboxed execution — network fully blocked
lion run -- node script.js
lion run -- python3 untrusted.py
lion run -- cargo test

# Full unrestricted internet access
lion run --net=full -- curl https://example.com
lion run --net=full -- npm install

# Domain-filtered access (built-in defaults cover npm, pip, cargo, GitHub)
lion run --net=allow -- npm install
lion run --net=allow -- pip install requests

# Add extra domains on top of proxy.toml
lion run --net=allow --domain my-registry.example.com -- npm install

# Use saved optional modules (X11 / Wayland / GPU / D-Bus / Fonts)
lion saved status
lion saved enable X11
lion run --optional X11 -- xterm
lion run --net=full --optional X11 --optional GPU -- ./firefox

# Mount additional paths read-only inside the sandbox
lion run --ro /usr/share/fonts -- python3 app.py

# Dry run — print the generated bwrap command without executing
lion run --dry-run -- ls -la

# Debug — enable verbose internal tracing logs
lion run --debug -- node index.js

# Run in single-terminal TUI mode (no extra monitor/perf terminals)
lion run --tui -- npm test
```

## First Real Examples

### Run untrusted Python code

```bash
lion run -- python3 suspicious_script.py
```

### Run tests without internet

```bash
lion run -- cargo test
lion run -- pytest
lion run -- npm test
```

### Install dependencies with filtered internet

```bash
lion run --net=allow -- npm install
lion run --net=allow -- pip install requests
lion run --net=allow -- cargo build
```

### Give full internet only when necessary

```bash
lion run --net=full -- curl https://example.com
lion run --net=full -- git clone https://github.com/user/repo.git
```

### Let the sandbox read extra host files

```bash
lion run --ro /usr/share/fonts -- python3 app.py
lion run --ro /etc/ssl -- curl https://example.com
```

### Dry-run the generated sandbox command

```bash
lion run --dry-run -- python3 app.py
```

## Network Modes

| Flag | Behaviour |
| --- | --- |
| *(default)* | Fully blocked — isolated network namespace |
| `--net=allow` | Only domains in `proxy.toml` reachable (built-in defaults if no file found) |
| `--net=full` | Unrestricted internet access |

### proxy.toml — domain allow-list

When using `--net=allow`, L.I.O.N looks for a domain allow-list in this order:

1. `./proxy.toml` in the current directory (project-local)
2. `~/.config/lion/proxy.toml` (user global)
3. **Built-in defaults** — npm, PyPI, Cargo, GitHub covered out of the box

To customise, drop a `proxy.toml` in your project root. See the repo's [`proxy.toml`](proxy.toml) for the full template.

### Proxy environment variables injected

`--net=allow` sets all of the following so every tool routes through the filter:

| Variable | Used by |
| --- | --- |
| `HTTP_PROXY` / `http_proxy` | curl, wget, Python requests, Go |
| `HTTPS_PROXY` / `https_proxy` | curl, wget, Python requests, Go |
| `ALL_PROXY` / `all_proxy` | curl, some Go tools |
| `npm_config_proxy` | **npm** (ignores `HTTP_PROXY`) |
| `npm_config_https_proxy` | **npm** |
| `PIP_PROXY` | pip |

## TUI Separation

Use `--tui` to render monitoring directly inside the current terminal:

```bash
lion run --tui -- cargo test
lion run --tui --net=allow -- npm install
```

In `--tui` mode, the current terminal becomes a Ratatui dashboard and no additional monitor/perf terminal windows are opened.

Without `--tui`, the existing multi-terminal behavior remains unchanged.

When a sandbox starts, L.I.O.N automatically opens two extra terminal windows:

- **Monitor window** — live access log (READ / WRITE / CREATE / DELETE / BLOCKED events)
- **Perf window** — real-time CPU and RAM bar graph with spark-line history

Both windows **close automatically** when the sandbox exits. If no supported terminal (`gnome-terminal` / `kitty`) is found, monitoring falls back to inline stderr.

## What the monitor shows

```text
╔══════════════════════════════════════════════════╗
║  LION MONITOR  ·  live sandbox events            ║
╚══════════════════════════════════════════════════╝
[LION] 20:14:03  ✅ READ    /home/user/project/src/main.rs
[LION] 20:14:03  ✏️  WRITE   /home/user/project/output.txt
[LION] 20:14:04  BLOCKED  /etc/shadow: Permission denied
[LION] 20:14:04  BLOCKED  /home/user/.ssh/id_rsa: Permission denied
[LION] monitor stopped
```

- **Green READ** — allowed file read (inotify on bind-mounted paths)
- **Yellow WRITE / Blue CREATE / Red DELETE** — filesystem mutations inside the sandbox
- **Red BLOCKED** — access attempt the sandbox denied (parsed from bwrap stderr)

The monitor stops cleanly within 50 ms of the sandboxed process exiting.

## lion.toml — per-project config

Drop a `lion.toml` in your project root to configure mount behaviour without CLI flags:

```toml
[sandbox]
project_access = "ro"   # "ro" | "rw"

[[mount]]
path = "~/.npmrc"
access = "ro"
```

See the repo's [`lion.toml`](lion.toml) for the full template.

## saved.toml — saved optional modules

Drop a [saved.toml](saved.toml) in your project root to keep reusable optional module sets.

Modules load when either:

- `state = 1` in saved.toml, or
- you pass `--optional <name>` for a single run.

Common commands:

```bash
lion saved status
lion saved enable X11
lion saved disable GPU
lion run --optional X11 -- xterm
```

Use the commented template inside [saved.toml](saved.toml) to add more modules.

### Minimal saved.toml example

```toml
[[modules]]
name = "MyData"
path = "/path/to/data"
state = 1
```

This means:

- module name is `MyData`
- host path `/path/to/data` is mounted when active
- `state = 1` makes it load automatically on every run

### Advanced saved.toml example

```toml
[[modules]]
name = "MyGuiModule"
env = ["DISPLAY", "XAUTHORITY"]
state = 0

[[modules.mounts]]
src = "/tmp/.X11-unix"
dst = "/tmp/.X11-unix"
mode = "rw"

[[modules.mounts]]
src = "${HOME}/.Xauthority"
dst = "${HOME}/.Xauthority"
mode = "ro"
```

This stays disabled by default, but can be used for one run with:

```bash
lion run --optional MyGuiModule -- xterm
```

### Recommended module workflow

For reusable local setups:

1. Put modules in [saved.toml](saved.toml)
2. Keep most of them at `state = 0`
3. Enable always-needed ones with `lion saved enable <name>`
4. Use `--optional <name>` for temporary one-off access

Example:

```bash
lion saved enable Fonts
lion run --optional X11 --optional GPU -- glxgears
```

## Common Workflows

### Node.js project

```bash
lion run --net=allow -- npm install
lion run -- npm test
lion run -- node index.js
```

### Python project

```bash
lion run --net=allow -- pip install -r requirements.txt
lion run -- pytest
lion run -- python3 main.py
```

### Rust project

```bash
lion run --net=allow -- cargo build
lion run -- cargo test
lion run -- cargo run
```

### GUI application

```bash
lion saved status
lion run --optional X11 --optional GPU -- xterm
lion run --optional Wayland --optional GPU -- your-gui-app
```

### Sandbox with project config

If you do not want to repeat CLI flags, keep project defaults in [lion.toml](lion.toml) and reusable modules in [saved.toml](saved.toml).

Typical layout:

- [lion.toml](lion.toml) → persistent bind mounts and project access mode
- [proxy.toml](proxy.toml) → allowed domains for `--net=allow`
- [saved.toml](saved.toml) → reusable optional module definitions

## Troubleshooting

### Command cannot access internet

- Use `--net=allow` for filtered internet
- Use `--net=full` for unrestricted internet
- Add missing domains to [proxy.toml](proxy.toml)

### GUI app does not open

- Check `lion saved status`
- Try `--optional X11`
- If hardware rendering is needed, add `--optional GPU`
- If the app uses desktop services, add `--optional D-Bus`

### A file is missing inside the sandbox

- Add it with `--ro /path`
- Or persist it in [lion.toml](lion.toml)
- Or create a reusable module in [saved.toml](saved.toml)

### A module path uses `${VAR}` but does not resolve

Module paths in [saved.toml](saved.toml) support `${VAR}` expansion from your host environment.
Examples:

```toml
src = "${HOME}/.Xauthority"
src = "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}"
```

Unset variables expand to an empty string, so verify that the relevant environment variable exists on the host first.

## Exit Codes

### L.I.O.N own exit codes

These are emitted by `lion` itself when sandbox setup fails — the sandboxed command never ran.

| Code | Meaning |
| --- | --- |
| `0` | Success — sandbox ran and command exited cleanly |
| `1` | Internal lion error (bug or unexpected failure) |
| `125` | Sandbox setup failed (bwrap couldn't start) |
| `126` | Command found but not executable (`chmod +x` needed) |
| `127` | Command not found inside the sandbox |

### Sandboxed program exit codes (passed through)

When the program *inside* the sandbox fails, its exit code is forwarded directly. Common ones you'll see:

| Code | Program | Meaning | Fix |
| --- | --- | --- | --- |
| `1` | any | Generic failure | Check program output |
| `2` | any | Misuse / bad arguments | Check command syntax |
| `6` | curl | Couldn't resolve host | Add `--net=allow` or `--net=full` |
| `7` | curl | Failed to connect | Add `--net=full` |
| `35` | curl | SSL handshake failed | Add `--net=full` |
| `46` | npm | Network error | Use `--net=allow` (sets npm proxy vars) |
| `52` | npm | Empty/bad proxy response | Proxy bug — update lion |
| `128+N` | any | Killed by signal N | Usually OOM or timeout |

Logs are always written to `~/.lion/logs/last-run.log` regardless of exit code.

## Limitations

- **Bubblewrap dependency** — requires `bwrap` on the host.
- **Ubuntu/Debian paths** — hardcoded mounts (`/usr`, `/lib`) work on most distros but need adjustment on NixOS.
- **No seccomp filter** — syscalls are not filtered; namespace isolation is the primary barrier.
- **No resource limits** — CPU and RAM are unrestricted (visible in the perf monitor but not capped).
- **Snap packages incompatible** — `snap-confine` requires `CAP_MAC_ADMIN`, stripped in rootless namespaces.
- **Linux only** — fundamentally tied to Linux namespaces.

## Architecture

```text
src/
  sandbox_engine/
    builder.rs      — namespace flags, synthetic root, hardening (die-with-parent, clearenv)
    environment.rs  — --clearenv + safe env allowlist
    mounts.rs       — bind-mount logic (ro/rw/dev)
    runner.rs       — orchestrates the full execution pipeline
    network.rs      — NetworkMode enum (None / Allow / Full)
    userns.rs       — pre-flight user namespace check
  monitor/
    mod.rs          — MonitorHandle: spawns stderr + inotify threads, separate terminal
    log.rs          — bwrap stderr parser (BLOCKED events)
    inotify.rs      — inotify watcher (READ/WRITE/CREATE/DELETE events)
    perf.rs         — CPU/RAM perf monitor (embedded Python, separate terminal)
  proxy/
    mod.rs          — embedded Python HTTP/HTTPS proxy, domain allow-list filtering
  config.rs         — lion.toml loader
  errors.rs         — structured LionError types
  logger.rs         — dual-target logging (stderr + ~/.lion/logs/last-run.log)
  install.rs        — one-time AppArmor setup
```

## Roadmap

- **Seccomp filter** — syscall allowlist for an additional layer of confinement.
- **Resource limits** — cgroup-based CPU and RAM caps via `--max-cpu` / `--max-mem`.
- **Profile system** — `lion expose / unexpose` to manage a persistent `~/.config/lion/profile.toml`.
