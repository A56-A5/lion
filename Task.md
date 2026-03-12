# L.I.O.N — Hackathon Target (Revised Direction)

> **Product**: A zero-configuration Linux sandbox where you can see exactly what a program tries to access, control what it can reach, and manage everything from a live TUI.
>
> **Killer demo**: Run a malicious npm package inside L.I.O.N. Watch the TUI show every blocked file read, every blocked network call, CPU/memory usage spiking — all in real time. Kill it with one keypress.

---

## Architecture Decisions (Settled)

### Decision 1 — Per-execution sandbox (NOT a shared service)

Every `lion run` creates a **fresh, independent sandbox** from scratch. When the program exits, the sandbox is destroyed completely.

**Why not a shared service model ("cordon run" spawning into a common namespace)?**

The shared service model defeats the entire security pitch. If two programs share a namespace, a compromise of one is a compromise of both. The isolation guarantee evaporates. Technically it also requires heavyweight namespace lifecycle management (refcounting, cleanup on crash, etc.) with no benefit.

**Why per-execution wins the hackathon:**
- The demo story is: "this malware ran, was completely isolated, tried everything, succeeded at nothing, and when it exited the cage was gone." That story is only possible with per-execution sandboxes.
- Fresh sandbox = zero persistent state that can be poisoned between runs.
- Simpler implementation — no daemon, no IPC for namespace sharing, no cleanup race conditions.
- Judges can intuitively understand "one run = one cage." A shared background service requires explanation.

**Rule**: Each `lion run` call is fully independent. Two simultaneous `lion run` calls each get their own namespace, their own monitor session, their own TUI instance. They do not interact.

---

### Decision 2 — Decoupled monitor as a separate program / branch

The monitoring subsystem (`src/monitor/`) is designed to be **completely decoupled** from the sandbox engine and buildable as a standalone program. This enables clean parallel development on a separate git branch that merges without conflicts.

**Architecture:**

```
lion run (sandbox process)
    │
    ├── spawns bwrap child
    ├── creates session socket: ~/.cache/lion/session-<uuid>.sock
    └── writes session manifest: ~/.cache/lion/session-<uuid>.json
         { pid, command, started_at, socket_path }

lion-monitor (separate binary or lib)
    │
    ├── discovers session by UUID or "latest"
    ├── connects to session socket
    ├── receives LogEntry JSON lines over the socket
    └── can output to: TUI panel | flat log file | stdout | external consumer

TUI (src/tui/)
    │
    └── embeds lion-monitor as a library crate, connects to same socket
```

**Why this decouples perfectly:**
- The socket is the contract. Format = newline-delimited JSON `LogEntry` structs.
- Sandbox engine just writes to the socket — it does not import or know about TUI or monitor code.
- Monitor crate just reads from the socket — it does not import sandbox_engine.
- TUI imports monitor as a library, but monitor does NOT import TUI.
- The `monitor` branch can develop the socket protocol and `lion-monitor` binary independently. The only merge surface is the `LogEntry` struct definition (one file: `src/monitor/types.rs`).

**Session socket protocol:**
- Each log event is one line of JSON followed by `\n`
- The socket stays open for the lifetime of the sandbox run
- On sandbox exit, lion writes a final `{"event":"session_end","exit_code":N}` line and closes the socket
- Monitor clients handle EOF gracefully (show STOPPED state)

**`lion monitor <uuid>`** — attach to a running session by ID and stream its log to stdout. Works independently of the TUI. Useful for piping into `grep`, `jq`, logging to disk, etc.

**Branch strategy:**
- `main` — defines `LogEntry` in `src/monitor/types.rs`, implements socket writer in sandbox engine
- `monitor` branch — implements `lion-monitor` binary, socket reader, inotify watchers, bwrap stderr parser
- `tui` branch — implements ratatui UI, imports monitor as reader
- Merge order: `monitor` → `main`, then `tui` → `main`. Zero conflicts because each owns disjoint files.

---

## The 5 Pillars

```
1. Sandbox Core         — bwrap with hardened flags (mostly done)
2. Exposure Control     — single profile, live add/remove access
3. Domain Filter        — proxy-based network control per domain
4. Access Logging       — decoupled monitor binary + inotify + bwrap stderr
5. Live TUI             — connects to monitor socket, renders everything
```

---

## TASK 1 — Harden the sandbox core [DONE]

**Priority**: 🔴 CORE | **Time**: 20 min

**What to add to `builder.rs`**:

- `--die-with-parent` — if the lion process dies, the sandbox dies too. Without this, orphan sandboxed processes keep running after lion exits.
- `--hostname lion` — sets a fake hostname inside the sandbox. Small detail, big visual impact in demo (`hostname` returns `lion`).
- `--new-session` — detaches the sandbox from your terminal session, preventing it from sending signals to the host shell.
- `--dir` stubs before bind mounts — creates the directory structure explicitly (`/usr`, `/bin`, `/lib`, `/tmp`, `/run`) so the synthetic root looks intentional, not accidental.

These go into the existing `bwrap.args([...])` call in `builder.rs`. Order matters: `--tmpfs /` and `--dir` stubs must come before any `--ro-bind` mounts.

**Files to modify**: `src/sandbox_engine/builder.rs`

---

## TASK 2 — Single profile with live exposure control [DONE]

**Priority**: 🔴 CORE | **Time**: 1.5 hours

### What this is

Instead of multiple named profiles, there is exactly **one active profile** at `~/.config/lion/profile.json`. The user modifies it with simple commands:

```
lion expose /home/user/projects     add a writable path
lion unexpose /home/user/projects   remove it
lion expose --network               enable network module
lion expose --gpu                   enable GPU module
lion unexpose --network             disable network
lion status                         print current exposure state
```

The profile is just a JSON file with two fields: a list of enabled modules and a list of custom paths. The sandbox reads it fresh on every `lion run`.

### The profile file structure

Two fields only:
- `modules` — list of capability names that are active (e.g. `["gpu", "wayland"]`)
- `custom_paths` — list of user-added directories that get mounted read-write

Modules available: `gpu`, `wayland`, `x11`, `audio`, `network`, `fonts`.

Base system paths (`/usr`, `/bin`, `/lib`, `/etc`) are always mounted — they are NOT part of the profile. The profile only controls optional capabilities on top of that.

### Module definitions (embed in binary)

Create `src/config/modules.json` and embed it with `include_str!()` so it cannot be tampered with. Each module entry has:
- `mandatory` — bool (only `base` is mandatory, auto-added always)
- `mounts` — list of objects with `type` (ro-bind/bind/dev-bind), `src`, `dst`
- `runtime_sockets` — for wayland/audio, socket names found under `$XDG_RUNTIME_DIR`
- `env` — env var names this module needs forwarded (DISPLAY, WAYLAND_DISPLAY, etc.)

### Commands to implement

**`lion expose`** — reads current profile, adds the requested path or module, writes it back. Validates custom paths (cannot be `/`, `/home`, `/etc`, `/root`, `/var`, `/proc`, `/dev`, `/sys` — must be explicit subdirectory).

**`lion unexpose`** — reads profile, removes the requested item, writes back.

**`lion status`** — reads profile and prints a clean summary of what is currently exposed. Color-coded: green for enabled, red for blocked.

### Security: custom path validation

Before any custom path is saved to the profile, validate:
1. Must be an absolute path
2. Must not be a dangerous top-level dir (deny list above)
3. Must actually exist on disk

Reject with a clear error message if any check fails.

**Files to create**:
- `src/config/modules.json` + `src/config/mod.rs`
- `src/profile/mod.rs` — Profile struct with serde Serialize/Deserialize
- `src/profile/store.rs` — load/save profile from `~/.config/lion/profile.json`, default fallback if missing
- `src/profile/validator.rs` — custom path security checks
- `src/commands/expose.rs` — `lion expose` logic
- `src/commands/unexpose.rs` — `lion unexpose` logic
- `src/commands/status.rs` — `lion status` output

**Files to modify**: `src/main.rs` (add Expose, Unexpose, Status subcommands)

---

## TASK 3 — Module resolver [DONE]

**Priority**: 🔴 CORE | **Time**: 1.5 hours

**What**: A function that takes the current profile + the embedded `modules.json` and returns a flat, concrete list of what to actually mount and which env vars to forward.

**How it works**:
1. Always prepend `base` module (mandatory, even if not in profile)
2. For each module name in the profile, look it up in `modules.json`
3. For each mount in that module, check if the source path actually exists on disk — if yes, include it; if no, skip silently
4. For `runtime_sockets`, resolve against `$XDG_RUNTIME_DIR` — include only sockets that exist
5. Collect all env var names across all active modules
6. For each `custom_paths` entry — run validator, then include as a read-write bind

Output is a struct with four lists: `ro_mounts`, `dev_mounts`, `rw_mounts`, `env_vars`.

**If mandatory base module resolves to zero mounts**: abort with a clear error. This means something is seriously wrong on the machine.

This output struct is what `mounts.rs` and `environment.rs` consume. Neither of those files needs to know about modules or profiles — they just receive the resolved lists.

**Files to create**: `src/profile/resolver.rs`

---

## TASK 4 — Rewrite `mounts.rs` and `environment.rs` to use resolver output

**Priority**: 🔴 CORE | **Time**: 45 min

### `mounts.rs`

Replace `apply_system_mounts()` with `apply_profile_mounts()` which takes the resolved struct and loops through it. For each ro_mount, append `--ro-bind src dst`. For each dev_mount, append `--dev-bind src dst`. For each rw_mount, append `--bind src src`. Check existence before appending.

### `environment.rs`

Add `bwrap.env_clear()` as the very first thing — this strips all host env vars from the sandbox (API keys, shell secrets, aliases — all gone). Then allowlist only: `HOME`, `USER`, `LOGNAME`, `PATH`, `LANG`, `LC_ALL`, `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`. Then add whatever env vars the resolver says are needed for active modules.

The `gui: bool` parameter goes away entirely. GUI support is now just the `x11` or `wayland` module being active.

### `runner.rs`

Add the profile load + resolve stage between userns check and mount stage. Load profile from `~/.config/lion/profile.json`, resolve it, pass the resolved struct to both `apply_profile_mounts()` and `apply_environment()`. Remove `gui: bool` and `optional: Vec<String>` from the function signature.

**Files to modify**: `src/sandbox_engine/mounts.rs`, `src/sandbox_engine/environment.rs`, `src/sandbox_engine/runner.rs`

---

## TASK 5 — Access logging (decoupled monitor)

**Priority**: 🔴 CORE | **Time**: 1.5 hours
**Branch**: `monitor` (merges cleanly into main — owns `src/monitor/` entirely)

### Design principle

The monitor is **completely decoupled** from the sandbox engine. It runs as a separate concern, communicates via a Unix domain socket, and can be developed and tested independently. See Architecture Decision 2 above.

### What to track

We do NOT use ptrace (too complex, too slow, breaks multithreaded apps). Instead we track blocked accesses — which is more useful for security anyway, because allowed accesses are expected, blocked ones are the story.

### Session socket (sandbox engine side — in `main` branch)

When `lion run` launches:
1. Generate a session UUID
2. Create `~/.cache/lion/session-<uuid>.sock` (Unix domain socket, SOCK_STREAM)
3. Write `~/.cache/lion/session-<uuid>.json` with `{ pid, command, started_at, socket_path }`
4. Accept connections from monitor clients
5. Stream `LogEntry` JSON lines to all connected clients as events arrive
6. On exit: send `{"event":"session_end","exit_code":N}` and close

This is the ONLY interface between sandbox and monitor. One file: `src/monitor/types.rs` defines `LogEntry`. Both branches depend on this one shared type.

### LogEntry format (in `src/monitor/types.rs` — shared, in `main`)

```rust
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub status: Status,       // BLOCKED | ALLOWED
    pub event_type: EventType, // READ | WRITE | CONNECT | EXEC
    pub path: String,          // file path or domain:port
    pub source: Source,        // BwrapStderr | Inotify | Proxy
}
```

Serializes to one line of JSON per event. Max 1000 entries in TUI buffer.

### Monitor binary side (in `monitor` branch)

**Method 1: bwrap stderr parsing**

The sandbox engine captures bwrap's stderr pipe. It reads it line by line in a background thread and parses for:
- `Permission denied` — filesystem access blocked
- `No such file or directory` for sensitive paths (e.g. `~/.ssh`) — implies blocked access attempt
- Network connection refused (when `--unshare-net` active)

Each match becomes a `LogEntry` with `source: BwrapStderr` and `status: BLOCKED`, sent to the session socket.

**Method 2: inotify watchers on sensitive paths**

Before launching the sandbox, set up `inotify` watchers on high-value host paths:
- `~/.ssh/`
- `~/.gnupg/`
- `~/.config/`
- `~/Documents/`, `~/Downloads/`

Any IN_ACCESS, IN_OPEN, IN_MODIFY event fires immediately. This catches access attempts even when the sandbox engine doesn't report them. Each event becomes a `LogEntry` with `source: Inotify` and `status: BLOCKED` (because if the path isn't mounted into the sandbox, any access is by definition a breach attempt).

Each watcher runs in its own thread, sends events through an `mpsc` channel to the session socket writer.

### `lion monitor` command

```
lion monitor            # attach to most recent session, stream to stdout
lion monitor <uuid>     # attach to specific session
lion monitor --json     # raw JSON output (for piping to jq, logging, etc.)
```

This command is entirely standalone — it just reads the socket and prints. No sandbox engine, no TUI needed.

**Files to create**:
- `src/monitor/types.rs` — `LogEntry`, `Status`, `EventType`, `Source` (in `main`, shared)
- `src/monitor/mod.rs` — re-exports, session socket writer (in `main`)
- `src/monitor/inotify.rs` — inotify watcher threads (in `monitor` branch)
- `src/monitor/stderr.rs` — bwrap stderr parser (in `monitor` branch)
- `src/monitor/client.rs` — socket reader / `lion monitor` command (in `monitor` branch)

---

## TASK 6 — Proxy-based domain/network filtering

**Priority**: 🔴 CORE | **Time**: 2-3 hours

### Why proxy works

Inside the sandbox, `--unshare-net` blocks ALL network. But when network is enabled, everything goes through. A proxy sits between the sandbox and the internet — you control what domains are allowed.

### Architecture

```
sandbox process
    → HTTP_PROXY / HTTPS_PROXY env vars point to 127.0.0.1:PORT
    → proxy intercepts every outbound request
    → checks domain against allowlist in profile
    → ALLOW: forwards the request
    → BLOCK: returns 403, logs the attempt
```

The proxy runs as a **separate process on the host** (not inside the sandbox), launched by lion before starting the sandbox.

### What to implement

A minimal HTTP/HTTPS proxy in Rust using the `hyper` or `tiny-http` crate (or even a small Python script if time is short — `mitmproxy` in script mode can do this in ~20 lines).

The proxy needs to:
1. Listen on a local port (e.g. `8877`)
2. For HTTP: read the `Host` header, check against domain allowlist
3. For HTTPS: intercept the `CONNECT` method (this tells you the target domain), check it, either tunnel through or refuse
4. Log every decision: `ALLOWED connect api.github.com` or `BLOCKED connect evil.com`
5. Send log events through the same channel as the access logger so they appear in the TUI

### Domain rules in profile

Add an optional `allowed_domains` list to the profile:
- `[]` = no network (default)
- `["*"]` = all domains allowed
- `["api.github.com", "registry.npmjs.org"]` = only these domains

Commands:
```
lion expose --domain api.github.com     add a domain
lion unexpose --domain evil.com         remove a domain
```

### Wiring into sandbox

When network module is active, lion:
1. Starts the proxy process on a random port
2. Sets `HTTP_PROXY=http://127.0.0.1:PORT` and `HTTPS_PROXY=http://127.0.0.1:PORT` as `--setenv` args on bwrap
3. Does NOT use `--unshare-net` (sandbox has network, but only via proxy)
4. When sandbox exits, kills the proxy process

**Files to create**: `src/proxy/mod.rs`, `src/proxy/server.rs`
**Files to modify**: `src/sandbox_engine/builder.rs` (proxy launch when network active), `src/sandbox_engine/runner.rs`

---

## TASK 7 — Live TUI

**Priority**: 🔴 CORE | **Time**: 3-4 hours

### Crate to use: `ratatui`

ratatui is the standard Rust TUI library. Actively maintained. Add it to Cargo.toml.

### Layout

```
┌──────────────────────────────────────────────────────────────────┐
│  🦁 L.I.O.N   Status: RUNNING   PID: 4821   python malware.py   │
├─────────────────────────────┬────────────────────────────────────┤
│  EXPOSURE                   │  ACCESS LOG                        │
│                             │                                    │
│  ✅ /usr          (ro)      │  12:04:01 🔴 READ  ~/.ssh/id_rsa  │
│  ✅ /bin          (ro)      │  12:04:01 🔴 CONN  evil.com:443   │
│  ✅ /project      (rw)      │  12:04:02 ✅ READ  /usr/lib/...   │
│  ❌ /home         blocked   │  12:04:02 🔴 READ  /etc/passwd    │
│  ❌ network       off       │  12:04:03 🔴 CONN  c2server.ru    │
│                             │                                    │
│  [a] add path               │                                    │
│  [r] remove path            │                                    │
│  [n] toggle network         │                                    │
├─────────────────────────────┴────────────────────────────────────┤
│  RESOURCES                                                       │
│  CPU  ████████░░░░░░  54%    MEM  ███░░░░░░░░░  128MB           │
│  NET  ↑ 0 KB/s  ↓ 0 KB/s    PIDs: 3                            │
└──────────────────────────────────────────────────────────────────┘
  [q] kill sandbox   [c] clear log   [h] help
```

### Panels

**Top bar**: sandbox name, status (RUNNING/STOPPED), PID, command being run.

**Left panel — Exposure**: Shows all currently mounted paths and modules. Green checkmark = allowed. Red X = blocked. User can press `a` to add a path (opens inline input), `r` to remove, `n` to toggle network. Changes are written to profile.json immediately — but take effect on next `lion run` (note this to user).

**Right panel — Access Log**: Scrolling log of all events from the logger (Task 5) and proxy (Task 6). Color coded: green for allowed, red for blocked. Auto-scrolls to latest. User can press `c` to clear it.

**Bottom panel — Resources**: CPU%, memory usage in MB, network in/out KB/s, active PID count. Data sourced from `/proc/[PID]/stat` and `/proc/[PID]/status`. Updated every 500ms.

### How to launch

`lion run` command launches the sandbox AND opens the TUI simultaneously. bwrap runs in the background, TUI connects to the session socket and streams events. When bwrap exits, the socket sends `session_end` and TUI shows `STOPPED`, waiting for `q`.

`lion tui <uuid>` command opens TUI in monitoring mode and attaches to an existing session socket by UUID (for re-attaching to a running session).

`lion monitor` command streams the session log to stdout as plain text or JSON — no TUI, fully scriptable.

### Data flow

```
bwrap stderr parser ──┐
inotify watchers    ──┼──→ session socket (Unix domain) ──→ TUI reads on tick
proxy log events    ──┘                                  ──→ lion monitor reads on demand
```

The TUI connects to the session socket as a client. It reads `LogEntry` JSON from the socket in a background thread, pushes into `Arc<Mutex<VecDeque<LogEntry>>>`. Render tick reads from this buffer every 200ms. Resources panel reads `/proc` directly on each tick.

### Keyboard controls
- `q` — kill the sandbox (send SIGKILL to bwrap's PID), exit TUI
- `a` — open inline path input in exposure panel
- `r` — remove highlighted path in exposure panel
- `n` — toggle network module in profile
- `c` — clear access log
- Arrow keys — scroll access log
- `h` — show help overlay

**Files to create**: `src/tui/mod.rs`, `src/tui/app.rs` (state), `src/tui/ui.rs` (rendering), `src/tui/events.rs` (keyboard input handling)
**Files to modify**: `src/main.rs` (launch TUI from `lion run`), `Cargo.toml` (add `ratatui`, `crossterm`)

---

## TASK 8 — Scanner (lion scan)

**Priority**: 🟡 OPTIONAL | **Time**: 1 hour

**What**: Detects which optional modules are available on the current machine and writes a sensible initial profile.

**How**: For each module in `modules.json`, check if any of its paths or sockets exist. Print results. Ask user which ones to enable. Write profile.

This is nice to have for first-run UX but not needed for the demo. The demo profile can be written manually.

**Files to create**: `src/scanner/detector.rs`, `src/scanner/mod.rs`
**Files to modify**: `src/main.rs` (add `Scan` subcommand)

---

## TASK 9 — Demo script

**Priority**: 🔴 CORE | **Time**: 30 min

**What**: `demo.sh` in repo root. A scripted sequence that shows the entire product story in under 3 minutes.

**Sequence**:
1. Show `lion status` — clean output of current exposure
2. Run `lion run cat ~/.ssh/id_rsa` → TUI opens, shows BLOCKED in log
3. Run `lion run ps aux` → shows almost nothing (PID isolation)
4. Run `lion run curl google.com` → TUI shows BLOCKED network attempt
5. Run `lion expose --network`, then `lion expose --domain google.com`
6. Run `lion run curl google.com` → TUI shows ALLOWED
7. Create `malware.js` (tries to read `~/.ssh/id_rsa`, connects to `evil.com`, deletes `~/Documents`)
8. Run `lion run node malware.js` → TUI shows all three attempts BLOCKED, Documents untouched

The malware script is the closing punch. Write it, commit it to the repo, reference it in slides.

**Files to create**: `demo.sh`, `demo/malware.js`, `demo/malware.py`

---

## TASK 10 — Update Cargo.toml

**Priority**: 🔴 CORE | **Time**: 5 min

**Add**:
- `serde_json = "1.0"` — JSON for modules.json and profile
- `ratatui = "0.29"` — TUI framework
- `crossterm = "0.28"` — terminal input/output backend for ratatui
- `inotify = "0.11"` — filesystem event watching for access logging
- `tokio = { version = "1", features = ["full"] }` — async runtime for proxy
- `hyper = "1"` or `tiny-http = "0.12"` — for the proxy server

**Remove**: `toml = "0.8"` (profiles are JSON, not TOML — this is unused)

---

## TASK 11 — Remove dead code

**Priority**: 🔴 CORE | **Time**: 20 min

- `mounts.rs` — delete `apply_system_mounts()` entirely
- `environment.rs` — delete `if gui { ... }` block and `gui: bool` parameter
- `runner.rs` — remove `gui: bool`, `optional: Vec<String>` from `run_sandboxed()` signature
- `builder.rs` — remove old network resolv.conf bind block
- `main.rs` — remove `--gui` and `--optional` flags from `Run` subcommand

---

## Final File Structure

```
src/
    main.rs                  (modified: new subcommands, no --gui)
    install.rs               (unchanged)

    sandbox_engine/
        mod.rs               (unchanged)
        runner.rs            (modified: loads profile + resolver, creates session socket, launches TUI)
        builder.rs           (modified: new flags, proxy launch when network on)
        mounts.rs            (modified: apply_profile_mounts)
        environment.rs       (modified: env_clear, module-driven vars)
        userns.rs            (unchanged)

    profile/
        mod.rs
        store.rs             (load/save ~/.config/lion/profile.json + default fallback)
        validator.rs         (custom path security checks)
        resolver.rs          (Profile + modules.json → ResolvedProfile)

    config/
        mod.rs               (include_str! embed)
        modules.json         (module definitions: base, gpu, wayland, x11, audio, network)

    commands/
        expose.rs            (lion expose logic)
        unexpose.rs          (lion unexpose logic)
        status.rs            (lion status output)
        mod.rs

    monitor/
        types.rs             (LogEntry, Status, EventType, Source — SHARED, in main branch)
        mod.rs               (session socket writer — in main branch)
        inotify.rs           (inotify watcher threads — monitor branch)
        stderr.rs            (bwrap stderr parser — monitor branch)
        client.rs            (socket reader, lion monitor command — monitor branch)

    proxy/
        server.rs            (HTTP/HTTPS intercepting proxy)
        mod.rs

    scanner/
        detector.rs          (detect available modules, write initial profile)
        mod.rs

    tui/
        app.rs               (TUI state struct — connects to session socket as client)
        ui.rs                (ratatui rendering)
        events.rs            (keyboard handling)
        mod.rs

demo/
    malware.js               (evil npm package simulation)
    malware.py               (evil python script simulation)

demo.sh                      (full demo sequence)
```

---

## Branch Strategy

```
main
 ├── sandbox_engine/ (core)
 ├── profile/ + config/ (exposure control)
 ├── src/monitor/types.rs  ← the only shared contract
 └── src/monitor/mod.rs    ← session socket writer

monitor branch (merges into main first)
 └── src/monitor/inotify.rs, stderr.rs, client.rs
     + lion monitor subcommand in main.rs

tui branch (merges after monitor)
 └── src/tui/ (connects to session socket as client)
     + lion tui subcommand in main.rs

proxy branch (merges independently)
 └── src/proxy/
```

---

## Build order (what to do first)

```
Hour 1      Task 10 (Cargo.toml) + Task 1 (builder flags)
Hour 2      Task 2 (modules.json + profile struct + store.rs)
Hour 3      Task 3 (resolver) + Task 4 (mounts + env rewrite)
Hour 4      Task 4 continued + wire into runner.rs
Hour 5      Task 2 continued (expose/unexpose/status commands)
Hour 6      Task 5 (monitor types + session socket in main, then inotify/stderr in monitor branch)
Hour 7-8    Task 6 (proxy — start with HTTP only, add HTTPS tunnel if time)
Hour 9-10   Task 7 (TUI — layout first, wire to session socket second)
Hour 11     Task 9 (demo script + malware files)
Hour 12     Polish, test demo 5 times, fix crashes

Parallel track (separate person, monitor branch):
  Hour 3-5    Implement lion-monitor: socket client, inotify watchers, stderr parser
  Hour 6      Test lion monitor <uuid> standalone — works without TUI
  Hour 7      Merge monitor → main, TUI branch connects to socket
```

---

## The pitch (one paragraph)

> Developers run untrusted code every day — npm installs, pip packages, AI-generated scripts. Any of them can read your SSH keys, phone home, or delete your files. L.I.O.N wraps any command in a Linux namespace sandbox and shows you — in real time — every file it tried to access, every domain it tried to reach, and every attempt it blocked. You control exactly what it can see. One command. Zero configuration. Works on every Linux distro.

**Keywords for slides**: Linux namespaces · principle of least privilege · zero-trust execution · filesystem isolation · process isolation · supply chain attack defense
