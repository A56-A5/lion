# WHAT IS L.I.O.N

## Product identity

**L.I.O.N = Lightweight Isolated Orchestration Node.**

L.I.O.N is a **per-execution Linux sandbox runner** for untrusted or risky commands.
Every `lion run` creates a fresh Bubblewrap namespace cage, runs one command, streams live security telemetry, then destroys the cage.

---

## Technical introduction (how it is built)

### Core system design

- **Language**: Rust (CLI, sandbox orchestration, telemetry pipeline).
- **Isolation backend**: Bubblewrap (`bwrap`) as the namespace/mount engine.
- **OS model**: Linux namespaces + bind mounts + synthetic root filesystem.
- **Execution model**: one command per disposable sandbox lifecycle.

### What we use for what

- **Command-line UX**: `clap` for subcommands and flags (`run`, `saved`, `install`).
- **Sandbox assembly**: Rust builder that generates strict `bwrap` arguments.
- **Filesystem telemetry**: `inotify` watchers on exposed host paths.
- **Blocked-attempt detection**: parser over sandbox stderr stream.
- **Network allow mode**: embedded Python stdlib proxy process (domain allowlist, no extra pip deps).
- **Live dashboard**: `ratatui` + `crossterm` for in-terminal TUI.
- **Perf metrics**: `/proc` parsing across process tree (CPU, RSS, threads, I/O).
- **Configuration**: TOML-based project/global configs (`lion.toml`, `saved.toml`, `proxy.toml`).
- **Logging**: structured tracing to stderr and `~/.lion/logs/last-run.log`.

### Runtime pipeline (high level)

1. Preflight checks (`bwrap` present, userns allowed).
2. Load and merge global + project config.
3. Build minimal synthetic root and explicit mounts.
4. Apply env wipe and safe allowlist.
5. Start optional proxy if network mode is `allow`.
6. Launch sandboxed command.
7. Stream monitor + perf events (TUI or split terminals).
8. Tear everything down on exit.

---

## The problem it solves

Developers regularly run commands they do not fully trust:

- `npm install`, `pip install`, `cargo build` with third-party code
- generated scripts from AI/tools
- unfamiliar repos and build scripts

The default host terminal gives these commands too much implicit access (files, tokens, network, desktop session).

L.I.O.N fixes this by making risky execution **isolated, inspectable, and disposable** without requiring Dockerfiles or VM setup.

---

## What the product does (today)

### 1) Disposable execution sandbox

- Unshares user, PID, IPC, UTS, cgroup namespaces by default.
- Uses synthetic root (`tmpfs /`) and only bind-mounts explicit paths.
- Kills sandbox with parent (`--die-with-parent`) to avoid orphaned escapees.
- Uses clean session and fake hostname (`lion`).

### 2) Filesystem exposure control

- Project can run read-only or read-write (`lion.toml`).
- `src/` can be forcibly re-overlaid as read-only even when project root is rw.
- Extra host paths can be added read-only via CLI (`--ro`) or config (`lion.toml`).
- Optional modules (`saved.toml` + `--optional`) expose only selected capabilities.

### 3) Environment secret minimization

- Starts with `--clearenv` (full wipe).
- Re-injects only allowlisted baseline env vars plus module-specific env when enabled.

### 4) Network policy modes

- `none` (default): isolated net namespace, no network.
- `full`: unrestricted host networking.
- `allow`: embedded local HTTP/HTTPS proxy with domain allowlist (`proxy.toml` + built-in defaults).
- Auto-injects proxy env vars for common tooling (`HTTP(S)_PROXY`, npm, pip).

### 5) Runtime observability

- Live file activity monitoring with inotify (READ/WRITE/CREATE/DELETE).
- Live blocked-attempt visibility via stderr parsing (`Permission denied`, etc.).
- Real-time CPU/RAM/process-tree telemetry (TUI or separate perf terminal).
- Unified TUI mode combines events, output, and perf in one terminal dashboard.

---

## What L.I.O.N objectively aces at (fact-based)

### 1) Safe defaults with low user effort

- Default network is blocked (`--net=none`) without extra setup.
- Full env wipe is automatic (`--clearenv`) and not opt-in.
- Sandbox is per-run and disposable by design.

**Why this matters:** teams get meaningful risk reduction from the first command, not after writing policies.

### 2) Precision exposure control instead of all-or-nothing trust

- Synthetic root starts empty; only explicit bind mounts are visible.
- Project access is configurable (`ro`/`rw`) and `src` can stay read-only even in rw project mode.
- Optional modules are explicit (`saved.toml` state + `--optional` one-shot activation).

**Why this matters:** developers can grant exactly what a workflow needs, instead of exposing full home/system context.

### 3) Observable security, not blind security

- inotify captures file-level READ/WRITE/CREATE/DELETE on exposed paths.
- stderr parsing surfaces blocked attempts in real time.
- `/proc`-based perf collector provides CPU/RAM/process-tree activity while command runs.

**Why this matters:** users can verify behavior live, not just trust that isolation happened.

### 4) Practical network governance for dev tooling

- `allow` mode routes HTTP/HTTPS through an embedded domain-filter proxy.
- Supports project-level + global allowlists with built-in defaults for common package ecosystems.
- Injects proxy environment variables used by common tooling (curl/wget/requests/npm/pip).

**Why this matters:** enables dependency installs and API access without opening unrestricted internet by default.

### 5) Better day-to-day UX for command sandboxing

- Single-command model: `lion run -- <cmd>`.
- Supports both terminal-native TUI mode and split monitor/perf terminal mode.
- Automatic teardown (`--die-with-parent`) limits leftover background process risk.

**Why this matters:** security controls are usable in daily workflows, not only in security specialist flows.

---

## What L.I.O.N can prevent / reduce

- Accidental host contamination from untrusted scripts.
- Direct access to host files that were never mounted.
- Environment-variable secret leakage (tokens/keys) via default env wipe.
- Default outbound network calls in `--net=none` mode.
- Non-allowlisted HTTP/HTTPS destinations in `--net=allow` mode.
- Silent tampering of source code via `src/` read-only overlay.

---

## What L.I.O.N does **not** do (important realism)

- Not a VM or hypervisor boundary.
- No seccomp syscall filter yet.
- No built-in CPU/RAM hard caps yet (it monitors usage, does not enforce limits).
- No full protocol filtering beyond HTTP/HTTPS proxy policy in allow mode.
- No guaranteed protection if user enables broad risky mounts/modules (e.g., rw home, D-Bus, GPU, X11).
- Linux-only (Bubblewrap + namespaces required).

---

## Product differentiation (pitch-ready)

### Differentiation we can claim without overreach

- **Command-level disposable isolation** tailored to developer command execution, not long-lived service containers.
- **Isolation + telemetry in one product path** (filesystem events, blocked attempts, perf/process view).
- **Source-protection overlay capability** (`src` read-only overlay) while preserving writable build outputs when needed.
- **Policy layering model** (global defaults + project config + per-run override) for controlled flexibility.
- **Filtered-network mode for package workflows** instead of binary "off/on internet" only.

### Bottom-line unique value

L.I.O.N is strongest where teams need to run risky commands quickly **with evidence**:

1. isolate execution,
2. minimize exposure,
3. observe behavior live,
4. destroy context immediately.

---

## Competitive framing (realistic)

- **Vs Docker/containers**: optimized for one-off command hardening without image authoring or container lifecycle overhead.
- **Vs generic sandbox wrappers**: differentiated by integrated real-time telemetry + TUI + domain-filter network mode in the same workflow.
- **Vs full VMs**: lower friction and faster execution loop; trade-off is weaker isolation boundary than hardware virtualization.

---

## Ideal users / use cases

- Developers running third-party package manager commands.
- Teams evaluating unknown repos/build scripts.
- CI-like local safety checks before trusting tooling.
- Security-minded desktop Linux workflows that still need selective GUI/network access.

---

## Judge FAQ (direct answers)

### Q1) Is this a VM-level security boundary?
No. L.I.O.N is a strong **process/container-style isolation layer**, not hardware virtualization.

### Q2) Why Rust + Bubblewrap instead of writing a sandbox from scratch?
Rust gives safer systems orchestration; Bubblewrap is a proven Linux sandbox primitive. We compose and harden it for developer workflows.

### Q3) What is the strongest security value today?
Default-deny execution with explicit exposure control + env wipe + disposable lifecycle + live visibility of both allowed and blocked behavior.

### Q4) What is your unique feature set vs similar tools?
The practical combo: per-command disposable sandboxing, domain-filtered network mode, source-protection overlay, and real-time multi-signal telemetry in one UX.

### Q5) What assumptions does your threat model make?
Linux host, rootless namespace support, and users not intentionally over-exposing dangerous mounts/modules. It is designed for risky dev workloads, not hostile kernel escape research.

### Q6) What are current gaps?
No seccomp enforcement yet, no hard resource caps yet, and non-HTTP protocols are not policy-filtered in allow mode.

### Q7) How production-ready is it now?
Ready for developer and team workflows where command-level isolation and visibility are needed immediately. Enterprise-hardening roadmap includes seccomp and resource governance.

### Q8) Why should judges care?
Because this solves a high-frequency real problem: developers execute untrusted commands daily. L.I.O.N turns that from blind trust into controlled, observable execution with minimal friction.

### Q9) What proof points can we cite in a demo?

- `--clearenv` behavior: secrets in host env are not visible unless explicitly reintroduced.
- `--net=none`: network-dependent commands fail by default.
- `--net=allow`: non-allowlisted domains are blocked while allowlisted package domains succeed.
- `src` overlay: writes to source files are prevented when source protection is enabled.
- TUI monitor: real-time file events + blocked attempts + perf/process metrics during the same run.

---

## End goal

Make “safe-by-default command execution” normal for developers:

- default to zero trust,
- expose only what is needed,
- observe everything in real time,
- tear down completely after each run.

Long-term direction already implied in roadmap: add seccomp policy, resource limits, and stronger profile automation while preserving the same one-command UX.
