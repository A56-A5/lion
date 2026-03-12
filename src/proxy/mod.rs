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

    tracing::info!("No proxy.toml found — all domains blocked. Add ~/.config/lion/proxy.toml to set defaults.");
    ProxyConfig::default()
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
/// Uses only stdlib: socket, threading, http.server — no pip required.
const PROXY_SCRIPT: &str = r#"
import socket
import threading
import sys
import os

# Handle wildcard or comma-separated list
raw_allowed = sys.argv[1].split(",")
if "*" in raw_allowed:
    ALLOWED = None
else:
    ALLOWED = set(d.strip().lower() for d in raw_allowed if d.strip())

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
        parts = first_line.split()
        if len(parts) < 2:
            return
        method, target = parts[0], parts[1]

        # HTTPS — CONNECT tunnel
        if method == "CONNECT":
            domain = target  # e.g. "api.github.com:443"
            if is_allowed(domain):
                log(True, domain)
                host_port = domain.rsplit(":", 1)
                host = host_port[0]
                port = int(host_port[1]) if len(host_port) > 1 else 443
                remote = socket.create_connection((host, port), timeout=10)
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
                host_port = host_header.split(":")
                host = host_port[0]
                port = int(host_port[1]) if len(host_port) > 1 else 80
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
