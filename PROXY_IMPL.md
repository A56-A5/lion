# Network Proxy — Implementation Guide

> **Approach**: Python stdlib proxy embedded as a string in Rust, spawned as a subprocess.
> No pip installs. No new crates. Python is on every Linux machine.
> **Estimated time**: 45 minutes.
> **Git conflict risk**: LOW — you own one new file (`src/proxy/mod.rs`) and make
> two small targeted insertions in `runner.rs` and `main.rs`.

---

## Files You Will Touch

| File | What you do | Conflict risk with TUI branch |
|---|---|---|
| `src/proxy/mod.rs` | CREATE — owns everything | None — new file |
| `src/sandbox_engine/runner.rs` | INSERT 4 lines before "// 6. Execute" and 1 line after `child.wait()` | Low — different line region than TUI |
| `src/sandbox_engine/environment.rs` | INSERT 2 `--setenv` lines inside existing fn | None — TUI doesn't touch this |
| `src/main.rs` | ADD `--domains` arg to Run subcommand | Low — separate field in struct |

---

## Step 1 — Create `src/proxy/mod.rs`

Create this file in full. It is entirely self-contained.

```rust
//! Embedded Python proxy for domain-level network filtering.
//!
//! Spawns a Python subprocess running a minimal HTTP/HTTPS proxy.
//! The proxy reads a domain allowlist, blocks all others with 403,
//! and logs every decision to stderr (picked up by the monitor).

use std::io::Write;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};

/// The Python proxy script embedded at compile time.
/// Uses only stdlib: socket, threading, http.server — no pip required.
const PROXY_SCRIPT: &str = r#"
import socket
import threading
import sys
import os

ALLOWED = set(sys.argv[1].split(",")) if sys.argv[1] != "*" else None
PORT    = int(sys.argv[2])

def log(status, domain):
    tag   = "\033[1;32mALLOWED\033[0m" if status else "\033[1;31mBLOCKED\033[0m"
    print(f"[LION-PROXY] {tag}  {domain}", flush=True)

def is_allowed(domain):
    host = domain.split(":")[0].lower()
    if ALLOWED is None:
        return True
    return host in ALLOWED or any(host.endswith("." + d) for d in ALLOWED)

def pipe(src, dst):
    try:
        while True:
            data = src.recv(4096)
            if not data:
                break
            dst.sendall(data)
    except:
        pass
    finally:
        try: src.close()
        except: pass
        try: dst.close()
        except: pass

def handle(conn):
    try:
        data = b""
        while b"\r\n" not in data:
            chunk = conn.recv(4096)
            if not chunk:
                return
            data += chunk

        first_line = data.split(b"\r\n")[0].decode(errors="replace")
        method, target, *_ = first_line.split()

        # HTTPS — CONNECT tunnel
        if method == "CONNECT":
            domain = target  # e.g. "api.github.com:443"
            if is_allowed(domain):
                log(True, domain)
                host, port = domain.rsplit(":", 1)
                remote = socket.create_connection((host, int(port)), timeout=10)
                conn.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
                t = threading.Thread(target=pipe, args=(remote, conn), daemon=True)
                t.start()
                pipe(conn, remote)
            else:
                log(False, domain)
                conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
        # HTTP — read Host header
        else:
            headers = data.split(b"\r\n")
            host_header = ""
            for h in headers[1:]:
                if h.lower().startswith(b"host:"):
                    host_header = h[5:].strip().decode(errors="replace")
                    break
            domain = host_header or target
            if is_allowed(domain):
                log(True, domain)
                # Simple forward: reconstruct and relay
                host = host_header.split(":")[0]
                port = int(host_header.split(":")[1]) if ":" in host_header else 80
                remote = socket.create_connection((host, port), timeout=10)
                remote.sendall(data)
                resp = b""
                while True:
                    chunk = remote.recv(4096)
                    if not chunk:
                        break
                    resp += chunk
                conn.sendall(resp)
                remote.close()
            else:
                log(False, domain)
                conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
    except Exception as e:
        pass
    finally:
        try: conn.close()
        except: pass

srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", PORT))
srv.listen(50)
print(f"[LION-PROXY] listening on 127.0.0.1:{PORT}", flush=True)
while True:
    try:
        conn, _ = srv.accept()
        threading.Thread(target=handle, args=(conn,), daemon=True).start()
    except:
        break
"#;

/// A running proxy process. Killed automatically on drop.
pub struct ProxyHandle {
    child: Child,
    pub port: u16,
}

impl ProxyHandle {
    /// Spawn the proxy with the given domain allowlist.
    ///
    /// `allowed_domains`:
    ///   - `[]`        → block everything (pass `"BLOCK_ALL"` internally — no allowed list)
    ///   - `["*"]`     → allow everything
    ///   - `["domain.com"]` → only that domain (and its subdomains)
    pub fn spawn(allowed_domains: &[String]) -> Result<Self, String> {
        let port = find_free_port().ok_or("no free port available")?;

        let domain_arg = if allowed_domains.is_empty() {
            "BLOCK_ALL".to_string()  // nothing will match, everything blocked
        } else {
            allowed_domains.join(",")
        };

        // Write the script to a temp file
        let script_path = std::env::temp_dir().join("lion_proxy.py");
        std::fs::write(&script_path, PROXY_SCRIPT)
            .map_err(|e| format!("failed to write proxy script: {e}"))?;

        let child = Command::new("python3")
            .arg(&script_path)
            .arg(&domain_arg)
            .arg(port.to_string())
            .stdout(Stdio::inherit())   // proxy logs go to lion's stdout (monitor sees them)
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("failed to spawn proxy: {e}"))?;

        // Wait briefly for the proxy to bind before bwrap starts
        std::thread::sleep(std::time::Duration::from_millis(150));

        Ok(ProxyHandle { child, port })
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Bind to port 0, let the OS assign a free port, return it.
fn find_free_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
}
```

---

## Step 2 — Register the module in `src/sandbox_engine/mod.rs`

Open `src/sandbox_engine/mod.rs` (or wherever crate modules are declared) and check if there is a top-level `pub mod proxy`. If not, add to `src/main.rs` at the top with the other `pub mod` lines:

```rust
pub mod proxy;
```

---

## Step 3 — Wire into `src/sandbox_engine/runner.rs`

### 3a. Add import at the top of the file
```rust
// add alongside existing use statements
use crate::proxy::ProxyHandle;
```

### 3b. Add `allowed_domains` parameter to `run_sandboxed`

Find the function signature:
```rust
pub fn run_sandboxed(
    cmd: Vec<String>,
    network_mode: crate::sandbox_engine::network::NetworkMode,
    dry_run: bool,
    gui: bool,
    _optional: Vec<String>,
    ro_paths: Vec<String>,
) -> Result<()> {
```

Change to:
```rust
pub fn run_sandboxed(
    cmd: Vec<String>,
    network_mode: crate::sandbox_engine::network::NetworkMode,
    dry_run: bool,
    gui: bool,
    _optional: Vec<String>,
    ro_paths: Vec<String>,
    allowed_domains: Vec<String>,   // ← ADD THIS
) -> Result<()> {
```

### 3c. Insert proxy spawn block

Find this comment in the file (just before "// 6. Execute"):
```rust
    bwrap.arg("--chdir").arg(&project_dir).arg("--").args(&cmd);

    if dry_run {
```

Insert the proxy block **between** those two lines:
```rust
    bwrap.arg("--chdir").arg(&project_dir).arg("--").args(&cmd);

    // Proxy: spawn before bwrap so the port is ready when the sandbox starts
    let _proxy: Option<ProxyHandle> = match network_mode {
        crate::sandbox_engine::network::NetworkMode::Http
        | crate::sandbox_engine::network::NetworkMode::Full => {
            match ProxyHandle::spawn(&allowed_domains) {
                Ok(p) => {
                    let proxy_url = format!("http://127.0.0.1:{}", p.port);
                    bwrap.arg("--setenv").arg("HTTP_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("HTTPS_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("http_proxy").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("https_proxy").arg(&proxy_url);
                    info!("Proxy running on port {} — domains: {:?}", p.port, allowed_domains);
                    Some(p)
                }
                Err(e) => {
                    warn!("Proxy failed to start: {} — continuing without proxy", e);
                    None
                }
            }
        }
        _ => None,
    };

    if dry_run {
```

`_proxy` lives until the end of the function — when `child.wait()` returns, `_proxy` is dropped and the proxy is killed automatically.

---

## Step 4 — Add `--domains` flag to `src/main.rs`

Find the `Run` subcommand struct fields (the block with `--net`, `--dry-run`, `--gui`):

```rust
        /// Mount a directory as read-only inside the sandbox (repeatable, e.g. --ro /home/user/docs).
        #[arg(long, value_name = "PATH")]
        ro: Vec<String>,
    },
```

Add one field after `ro`:
```rust
        /// Domains the proxy will allow through (requires --net=http or --net=full).
        /// Use '*' to allow all. Repeatable: --domain google.com --domain api.github.com
        #[arg(long = "domain", value_name = "DOMAIN")]
        domains: Vec<String>,
    },
```

Then find the `sandbox_engine::run_sandboxed(...)` call in `main()` and add `domains.clone()` as the last argument:
```rust
            sandbox_engine::run_sandboxed(
                cmd.clone(),
                net.clone(),
                *dry_run,
                *gui,
                optional.clone(),
                ro.clone(),
                domains.clone(),   // ← ADD THIS
            )
```

---

## Step 5 — Verify it builds

```bash
cargo build
```

---

## How to Use

```bash
# Block all network even when --net=full (proxy allows nothing)
lion run --net=full -- curl https://google.com

# Allow only specific domains
lion run --net=full --domain google.com --domain api.github.com -- curl https://google.com

# Allow everything (same as raw --net=full but with logging)
lion run --net=full --domain '*' -- curl https://google.com
```

---

## What the output looks like

```
[LION-PROXY] listening on 127.0.0.1:54231
[LION-PROXY] BLOCKED  evil.com:443
[LION-PROXY] ALLOWED  api.github.com:443
[LION-PROXY] BLOCKED  tracking.io:443
```

Each line is printed to stdout — if you wire the TUI's log panel to also capture stdout, these show up in the access log automatically alongside the inotify READ/BLOCKED events.

---

## Git conflict avoidance

- `src/proxy/mod.rs` — brand new file, zero conflicts
- `src/sandbox_engine/runner.rs` — you insert **one block** between `bwrap.arg("--chdir"...)` and `if dry_run`. TUI work inserts after `child.wait()`. No overlap.
- `src/main.rs` — you add one field to the `Run` struct and one argument to the call site. TUI work adds a new subcommand entirely (`Tui`). No overlap.

Safe to merge in either order.

---

## Demo script hook

```bash
# In demo/malware.py — the script just tries to connect, proxy blocks it
import urllib.request
try:
    urllib.request.urlopen("https://evil.com")
except Exception as e:
    print(f"network blocked: {e}")   # sandbox sees this, proxy logged it
```

Run it as:
```bash
lion run --net=full -- python3 demo/malware.py
# Output: [LION-PROXY] BLOCKED  evil.com:443
```
