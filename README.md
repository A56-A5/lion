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
- **Selective capabilities** — opt-in to network (`--net`) or GUI sockets (`--gui`) only when needed.

For a full breakdown of what is exposed vs. hidden, see [EXPOSURES.md](EXPOSURES.md).

## System Setup

L.I.O.N requires `bwrap` on your system.
On Ubuntu 24+ and other distros that restrict unprivileged user namespaces via AppArmor, run the one-time setup:

```bash
# Creates a targeted AppArmor profile specifically for bwrap
sudo lion install
```

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

# Allow GUI rendering (exposes X11/Wayland sockets + GPU)
lion run --gui -- xterm
lion run --net=full --gui -- ./firefox

# Mount additional paths read-only inside the sandbox
lion run --ro /usr/share/fonts -- python3 app.py

# Dry run — print the generated bwrap command without executing
lion run --dry-run -- ls -la

# Debug — enable verbose internal tracing logs
lion run --debug -- node index.js
```

## Network Modes

| Flag | Behaviour |
|---|---|
| *(default)* | Fully blocked — isolated network namespace |
| `--net=allow` | Only domains in `proxy.toml` reachable (built-in defaults if no file found) |
| `--net=full` | Unrestricted internet access |

### proxy.toml — domain allow-list

When using `--net=allow`, L.I.O.N looks for a domain allow-list in this order:

1. `./proxy.toml` in the current directory (project-local)
2. `~/.config/lion/proxy.toml` (user global)
3. **Built-in defaults** — npm, PyPI, Cargo, GitHub covered out of the box

To customise, drop a `proxy.toml` in your project root. See the repo's [`proxy.toml`](proxy.toml) for the full template.

## TUI Separation

When a sandbox starts, L.I.O.N automatically opens two extra terminal windows:

- **Monitor window** — live access log (READ / WRITE / CREATE / DELETE / BLOCKED events)
- **Perf window** — real-time CPU and RAM bar graph with spark-line history

Both windows **close automatically** when the sandbox exits. If no supported terminal (`gnome-terminal` / `kitty`) is found, monitoring falls back to inline stderr.

## What the monitor shows

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

## Limitations

- **Bubblewrap dependency** — requires `bwrap` on the host.
- **Ubuntu/Debian paths** — hardcoded mounts (`/usr`, `/lib`) work on most distros but need adjustment on NixOS.
- **No seccomp filter** — syscalls are not filtered; namespace isolation is the primary barrier.
- **No resource limits** — CPU and RAM are unrestricted (visible in the perf monitor but not capped).
- **Snap packages incompatible** — `snap-confine` requires `CAP_MAC_ADMIN`, stripped in rootless namespaces.
- **Linux only** — fundamentally tied to Linux namespaces.

## Architecture

```
src/
  sandbox_engine/
    builder.rs      — namespace flags, synthetic root, hardening (die-with-parent, clearenv)
    environment.rs  — --clearenv + safe env allowlist
    mounts.rs       — bind-mount logic (ro/rw/dev/gui)
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

- **Live TUI** — ratatui dashboard showing exposure panel, scrolling access log, CPU/memory gauges, and one-key sandbox kill.
- **Seccomp filter** — syscall allowlist for an additional layer of confinement.
- **Resource limits** — cgroup-based CPU and RAM caps via `--max-cpu` / `--max-mem`.
- **Profile system** — `lion expose / unexpose` to manage a persistent `~/.config/lion/profile.toml`.
