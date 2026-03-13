# L.I.O.N Demo Commands

Use this exact order for the demo recording:
1) path/file restriction via editor,
2) network restriction,
3) live read/write/blocked monitoring proof.

## Docs map

- [README.md](README.md) — feature overview
- [WHAT.md](WHAT.md) — pitch language + uniqueness points
- [Commands.md](Commands.md) — this demo runbook
- [EXPOSURES.md](EXPOSURES.md) — what should/should not be visible in sandbox

---

## 0) Build + quick sanity

```bash
cargo build --release
./target/release/lion --help
```

If required on Ubuntu 24.04+:

```bash
sudo ./target/release/lion install
```

---

## 1) Path and file restriction (text-editor focused)

### 1.1 Open editor inside sandbox (GUI demo)

```bash
./target/release/lion saved status
./target/release/lion run --optional X11 -- gnome-text-editor
```

What to show inside editor:
- Open a file from project path (allowed visibility).
- Try to save into restricted system location like `/etc/hosts` (should fail / blocked).
- Try browsing sensitive host path like `~/.ssh` (not normally exposed by default sandbox).

### 1.2 CLI fallback (if GUI editor unavailable)

```bash
./target/release/lion run --tui -- bash -lc 'echo ok > /tmp/lion-ok.txt && echo blocked >> /etc/hosts'
```

What to show:
- write to `/tmp` succeeds,
- write to `/etc/hosts` is blocked (read-only/system protected).

---

## 2) Network restriction demo (clear contrast)

### 2.1 Default: network blocked

```bash
./target/release/lion run -- curl -I https://example.com
```

### 2.2 Allow-list mode

```bash
./target/release/lion run --net=allow -- npm ping
./target/release/lion run --net=allow -- pip index versions requests
```

Optional one-off domain add:

```bash
./target/release/lion run --net=allow --domain example.com -- curl -I https://example.com
```

### 2.3 Full network

```bash
./target/release/lion run --net=full -- curl -I https://example.com
```

---

## 3) Live monitoring proof (read/write/delete/blocked + perf)

```bash
./target/release/lion run --tui -- bash -lc 'cat Cargo.toml >/dev/null; echo demo > /tmp/lion-demo.txt; echo more >> /tmp/lion-demo.txt; rm /tmp/lion-demo.txt; echo blocked >> /etc/hosts; sleep 2'
```

In TUI, point at:
- Access Log: READ / WRITE / CREATE / DELETE / BLOCKED
- Process Tree
- Exposed Paths + Active Modules
- Command Output panel
- CPU/RAM section

---

## 4) Source protection proof (`src` write restriction)

```bash
./target/release/lion run -- bash -lc 'echo "try-write" >> src/main.rs'
```

Explain: with source protection enabled (`src_access = "ro"`), direct source mutation attempts are denied.

---

## 5) Final closeout proof points

```bash
./target/release/lion run --dry-run -- ls -la
./target/release/lion saved status
tail -n 50 ~/.lion/logs/last-run.log
```

Final lines to say in demo:
- Path/file access is restricted by default.
- Network is restricted by default and can be safely opened in levels.
- L.I.O.N shows live behavior evidence, not just pass/fail.
- Each run is disposable; sandbox dies cleanly after command exit.
