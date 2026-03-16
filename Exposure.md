# Exposure Report and Security Analysis

This document provides a detailed description of the L.I.O.N isolation model, its restrictions, and an honest assessment of its security capabilities and limitations.

## Tool Rating: 7/10 (Developer Sandbox)

L.I.O.N is rated as a "High-Utility Developer Sandbox." It is excellent for protecting against accidental host contamination and common credential leakage, but it is not a replacement for a hardened Virtual Machine (VM) when dealing with actively malicious kernels or specialized escape exploits.

### Strengths
- Zero-effort environment scrubbing (clearenv).
- Real-time visibility into filesystem interactions.
- Low performance overhead compared to VMs.
- Prevents tampering with project source files through read-only overlays.

### Limitations
- Shared kernel: A kernel-level exploit can potentially escape the sandbox.
- Protocol restrictions: Network filtering is limited to HTTP/HTTPS for domain allow-lists.
- No hardware isolation: Does not provide the same boundary as a hypervisor.

---

## Technical Security Analysis

### Isolation Mechanism
L.I.O.N utilizes Linux namespaces (User, PID, Network, IPC, UTS, Cgroup) via `bubblewrap`. This creates a logical container where the process sees a synthetic root filesystem.

### Security Defenses
1. **Filesystem Isolation**: The sandbox root is a `tmpfs` (RAM-based) filesystem that starts empty. Host paths are only visible if explicitly bind-mounted.
2. **Credential Protection**: By default, `~/.ssh`, `~/.gnupg`, and browser profiles are never mounted.
3. **Environment Scrubbing**: The `--clearenv` flag ensures that host secrets (API keys, tokens) are not leaked into the sandboxed process's memory space.
4. **Source Code Shield**: Even when a project is granted write access, the `src/` directory is re-mounted as read-only. This is a critical defense against malicious build scripts.

### Known Security Gaps
- **Syscall Surface**: L.I.O.N does not currently implement `seccomp` filters. A sandboxed process can still make any system call allowed by the kernel.
- **Resource Exhaustion**: There are no hard limits on CPU or RAM usage, meaning a sandboxed process could still cause a Denial of Service (DoS) on the host.
- **Domain Filter Bypass**: The `--net=allow` mode uses a proxy-based filter. Specialized malware that does not respect proxy environment variables could potentially bypass this if other protocols are allowed.

---

## Explicit Restrictions

The following host directories and resources are **never** exposed by L.I.O.N, regardless of configuration:

- Private Keys: `~/.ssh/`
- Encryption Keys: `~/.gnupg/`
- Personal Data: `~/Documents/`, `~/Downloads/`, `~/Desktop/`
- Browsing History/Cookies: `~/.mozilla/`, `~/.config/google-chrome/`
- System Secrets: `/etc/shadow`, `/etc/sudoers`
- System Logs: `/var/log/`

---

## Honest Assessment

L.I.O.N is built for developers who need to run `npm install`, `pip install`, or unknown build scripts with a degree of confidence. It provides **Controlled Execution** and **High Observability**. 

If your threat model involves running code designed to exploit the Linux kernel or perform hardware-level attacks, you should use a dedicated VM. For everything else in a standard dev workflow, L.I.O.N provides a massive security upgrade over a raw terminal.
