//! Embedded Python proxy for domain-level network filtering.
//!
//! Spawns a Python subprocess running a minimal HTTP/HTTPS proxy.
//! The proxy reads a domain allowlist, blocks all others with 403,
//! and logs every decision to stderr (picked up by the monitor).

use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ProxyConfig {
    #[serde(default)]
    pub domains: Vec<String>,
}

/// Built-in default allow-list used when no proxy.toml is found anywhere.
/// Covers the most common package managers and code hosts.
const DEFAULT_DOMAINS: &[&str] = &[
    // npm
    "registry.npmjs.org", "npmjs.org", "nodejs.org",
    // pip / PyPI
    "pypi.org", "files.pythonhosted.org", "bootstrap.pypa.io",
    // Rust / Cargo
    "crates.io", "static.crates.io", "index.crates.io",
    // GitHub
    "github.com", "api.github.com", "raw.githubusercontent.com",
    "objects.githubusercontent.com", "codeload.github.com",
    // General
    "google.com", "www.google.com",
];

pub fn load_config(project_dir: &Path) -> ProxyConfig {
    // 1. Project-local proxy.toml (highest priority)
    let local = project_dir.join("proxy.toml");
    if local.exists() {
        return load_from_path(&local, "local");
    }

    // 2. Global config: ~/.config/lion/proxy.toml
    if let Ok(home) = std::env::var("HOME") {
        let global = std::path::PathBuf::from(home).join(".config/lion/proxy.toml");
        if global.exists() {
            return load_from_path(&global, "global");
        }
    }

    // 3. Built-in defaults — no proxy.toml needed for common workflows
    tracing::info!("No proxy.toml found — using built-in default domain list");
    ProxyConfig {
        domains: DEFAULT_DOMAINS.iter().map(|s| s.to_string()).collect(),
    }
}

fn load_from_path(path: &Path, label: &str) -> ProxyConfig {
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<ProxyConfig>(&contents) {
            Ok(cfg) => {
                tracing::info!("Loaded {} proxy.toml from {}", label, path.display());
                cfg
            }
            Err(e) => {
                tracing::warn!("proxy.toml parse error: {e} — using defaults");
                ProxyConfig::default()
            }
        },
        Err(e) => {
            tracing::warn!("Could not read proxy.toml: {e} — using defaults");
            ProxyConfig::default()
        }
    }
}

/// The Python proxy script embedded at compile time.
/// Uses only stdlib: socket, threading — no pip required.
const PROXY_SCRIPT: &str = r#"
import socket
import threading
import sys
import time

# Parse domain allow-list
raw_allowed = sys.argv[1].split(",")
if "*" in raw_allowed:
    ALLOWED = None  # allow everything
else:
    ALLOWED = set(d.strip().lower() for d in raw_allowed if d.strip())

PORT = int(sys.argv[2])

def ts():
    return time.strftime("%H:%M:%S")

def log(status, domain, reason=""):
    tag  = "\033[1;32mALLOWED\033[0m" if status else "\033[1;31mBLOCKED\033[0m"
    sfx  = f"  \033[90m({reason})\033[0m" if reason else ""
    print(f"[LION-PROXY]  {ts()}  {tag}  {domain}{sfx}", flush=True)

def extract_host(domain_or_url: str) -> str:
    """Return bare hostname from either 'host:port' or 'http://host/path'."""
    s = domain_or_url.strip().lower()
    if s.startswith("http://") or s.startswith("https://"):
        # strip scheme
        s = s.split("//", 1)[1]
        # strip path
        s = s.split("/")[0]
    # strip port
    return s.split(":")[0]

def is_allowed(domain_or_url: str):
    if ALLOWED is None:
        return True
    host = extract_host(domain_or_url)
    if host in ALLOWED:
        return True
    # subdomain match: api.github.com matches "github.com"
    if any(host.endswith("." + d) for d in ALLOWED):
        return True
    return False

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
        for s in (src, dst):
            try: s.shutdown(socket.SHUT_RDWR)
            except: pass
            try: s.close()
            except: pass

def recv_headers(conn):
    """Read from conn until we have the complete HTTP header block (ends with \\r\\n\\r\\n)."""
    data = b""
    while b"\r\n\r\n" not in data:
        chunk = conn.recv(4096)
        if not chunk:
            return None
        data += chunk
        if len(data) > 65536:   # guard against oversized headers
            return None
    return data

def handle(conn):
    try:
        data = recv_headers(conn)
        if not data:
            return

        first_line = data.split(b"\r\n")[0].decode(errors="replace")
        parts = first_line.split()
        if len(parts) < 2:
            return
        method, target = parts[0], parts[1]

        # ── HTTPS — CONNECT tunnel ───────────────────────────────────────────
        if method == "CONNECT":
            if is_allowed(target):
                log(True, target)
                host_part, _, port_str = target.rpartition(":")
                host = host_part or target
                port = int(port_str) if port_str.isdigit() else 443
                try:
                    remote = socket.create_connection((host, port), timeout=15)
                except Exception as e:
                    log(False, target, f"connect failed: {e}")
                    conn.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    return
                conn.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
                # any bytes after the CONNECT headers in our buffer → forward to remote
                header_end = data.find(b"\r\n\r\n") + 4
                leftover = data[header_end:]
                if leftover:
                    remote.sendall(leftover)
                t = threading.Thread(target=pipe, args=(remote, conn), daemon=True)
                t.start()
                pipe(conn, remote)
            else:
                log(False, target, "domain not in allow-list")
                conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")

        # ── HTTP — plain request ─────────────────────────────────────────────
        else:
            # Extract Host header (fallback to URL if absent)
            host_header = ""
            for line in data.split(b"\r\n")[1:]:
                if line.lower().startswith(b"host:"):
                    host_header = line[5:].strip().decode(errors="replace")
                    break

            domain = host_header or target
            if is_allowed(domain):
                log(True, domain)
                hp = host_header.split(":")
                host = hp[0]
                port = int(hp[1]) if len(hp) > 1 and hp[1].isdigit() else 80
                try:
                    remote = socket.create_connection((host, port), timeout=15)
                    remote.settimeout(15)
                except Exception as e:
                    log(False, domain, f"connect failed: {e}")
                    conn.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    return
                remote.sendall(data)
                # Stream response back to client (handles keep-alive chunked responses)
                t = threading.Thread(target=pipe, args=(remote, conn), daemon=True)
                t.start()
                pipe(conn, remote)
            else:
                log(False, domain, "domain not in allow-list")
                conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")

    except Exception as e:
        pass
    finally:
        try: conn.close()
        except: pass

srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", PORT))
srv.listen(100)
print(f"[LION-PROXY] ready  127.0.0.1:{PORT}  ({len(ALLOWED) if ALLOWED is not None else '*'} domains)", flush=True)
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

        let domain_arg = if allowed_domains.contains(&"*".to_string()) {
            "*".to_string()
        } else if allowed_domains.is_empty() {
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
            // Route proxy logs to stderr so the TUI access-log monitor picks them
            // up via [LION-PROXY] parsing. If we used stdout here, TUI mode would
            // funnel them into the command-output panel instead.
            .stdout(Stdio::inherit())
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
