# L.I.O.N

> Wrap any command in a hardened Linux sandbox. See every file it touches, every blocked attempt, in real time.

L.I.O.N is a per-execution filesystem sandbox for Linux using Bubblewrap (`bwrap`). Every `lion run` creates a **fresh, independent namespace cage** from scratch. When the program exits, the cage is gone.

## Features

- **Full namespace isolation** — PID, user, IPC, UTS, cgroup, and network namespaces, all unshared by default.
- **Synthetic root** — the sandbox starts with a blank `/`. Only whitelisted paths are bind-mounted in. Nothing leaks.
- **Complete environment wipe** — `--clearenv` runs first. `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, `NPM_TOKEN`, shell aliases — all invisible inside the cage.
- **No orphan processes** — `--die-with-parent` ensures the sandbox is killed the moment `lion` exits or crashes.
- **Fake identity** — sandbox reports hostname `lion`, detached from your real terminal session.
- **Live access monitor** — inotify watchers on mounted paths report every file read, write, create, and delete in real time. `bwrap` stderr is parsed simultaneously to catch blocked permission errors.
- **Source protection** — `src/` is re-mounted read-only even when the project root is read-write.
- **Selective capabilities** — opt-in to network (`--net`) or GUI sockets (`--gui`) only when needed.

For a full breakdown of what is exposed vs. hidden, see [EXPOSURES.md](EXPOSURES.md).

## System Setup

L.I.O.N requires `bwrap` to be installed on your system.
On Ubuntu 24+ and other distros that restrict unprivileged user namespaces via AppArmor, run the one-time setup:

```bash
# Creates a targeted AppArmor profile specifically for bwrap
sudo lion install
```

## Usage

```bash
# Basic sandboxed execution — network off, full isolation
lion run -- node script.js
lion run -- python3 malware.py
lion run -- cargo test

# Enable network access
lion run --net=full -- curl https://example.com
lion run --net=dns  -- dig google.com

# Domain-specific filtering (requires proxy)
lion run --net=full --domain google.com --domain api.github.com -- curl https://google.com

# Allow GUI rendering (exposes X11/Wayland sockets)
lion run --gui -- xterm

# Mount additional paths read-only inside the sandbox
lion run --ro /usr/share/fonts --ro /etc/ssl -- python3 app.py

# Multi-flag combination
lion run --net=dns --gui --ro ~/data -- vlc ~/data/video.mp4

# Dry run — print the generated bwrap command
lion run --dry-run -- ls -la

# Debug — enable verbose internal tracing logs
lion run --debug -- node index.js
```

## TUI Separation

By default, L.I.O.N attempts to launch a separate terminal window (`gnome-terminal` or `kitty`) for live monitoring. This keeps the primary terminal focused on your application's output while security events stream in a dedicated dashboard.

If no supported terminal is found, L.I.O.N falls back to inline monitoring in the same terminal.

## What the monitor shows

While the sandbox runs, L.I.O.N streams a live access log to stderr:

```
╔══════════════════════════════════════════════════╗
║  LION MONITOR  ·  live sandbox events            ║
╚══════════════════════════════════════════════════╝
[LION] 20:14:03  ✅ READ    /home/user/project/src/main.rs
[LION] 20:14:03  ✏️  WRITE   /home/user/project/output.txt
[LION] 20:14:04  BLOCKED  /etc/shadow: Permission denied
[LION] 20:14:04  BLOCKED  /home/user/.ssh/id_rsa: Permission denied
[LION] monitor stopped
```

- **Green READ** — allowed file access (inotify on bind-mounted paths)
- **Yellow WRITE / Blue CREATE / Red DELETE** — filesystem mutations inside the sandbox
- **Red BLOCKED** — access attempt that the sandbox denied (parsed from bwrap stderr)

The monitor stops cleanly within 50ms of the sandboxed process exiting.

## Limitations

- **Bubblewrap dependency** — requires `bwrap` on the host.
- **Ubuntu/Debian paths** — hardcoded mounts (`/usr`, `/lib`) work on most distros but will need adjustment on NixOS or heavily customized setups.
- **No seccomp filter** — syscalls are not filtered; namespace isolation is the primary barrier.
- **No resource limits** — CPU and RAM usage are currently unrestricted.
- **Snap packages incompatible** — Snap's `snap-confine` requires `CAP_MAC_ADMIN`, which is stripped in rootless user namespaces. Use native binaries instead.
- **Linux only** — fundamentally tied to Linux namespaces.

### Running Firefox (non-Snap)

```bash
# Download native binary from Mozilla, extract, then:
lion run --net=full --gui -- ./firefox
```

## Architecture

```
src/
  sandbox_engine/
    builder.rs      — namespace flags, synthetic root construction
    environment.rs  — --clearenv + safe allowlist
    mounts.rs       — bind-mount logic (ro/rw/dev)
    runner.rs       — orchestrates the full execution pipeline
    userns.rs       — pre-flight user namespace check
  monitor/
    mod.rs          — MonitorHandle: spawns stderr + inotify threads
    log.rs          — bwrap stderr parser (BLOCKED events)
    inotify.rs      — inotify watcher (READ/WRITE/CREATE/DELETE events)
```

## Roadmap

- **Live TUI** — ratatui dashboard showing exposure panel, scrolling access log, CPU/memory gauges, and one-key sandbox kill.
- **Proxy-based network filter** — intercept HTTP/HTTPS at the proxy level; allow/block by domain.
- **Profile system** — `lion expose / unexpose` commands to manage a persistent `~/.config/lion/profile.json`.
- **Scanner** — auto-detect available modules (GPU, Wayland, audio) on first run.
