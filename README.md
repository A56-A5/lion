# L.I.O.N

A lightweight, per-execution filesystem sandbox for Linux using Bubblewrap (`bwrap`).

## Features & Maximum Capability

L.I.O.N currently operates as a static, pre-hardcoded sandbox engine. It protects your development environment by isolating processes from the rest of your system.

- **Maximum Sandboxing**: Creates an entirely new Linux namespace where your command runs. It isolates user IDs, inter-process communication, Process Trees (PIDs), hostnames, and cgroups. 
- **Hardcoded System Mounts**: Exposes only basic standard paths read-only to guarantee executables work (`/usr`, `/bin`, `/lib`, `/lib64`, `/etc/alternatives`).
- **Source Protection**: Re-mounts your current working directory as read-write, but strictly restricts the `src/` folder to be read-only so tests cannot maliciously overwrite your code.
- **Selective Capabilities**: You can opt-in to expose network interfaces (`--network`) or GUI rendering sockets (`--gui`).

## Usage Guide

Execute any command safely isolated from your home directory:

```bash
# Basic sandboxed execution (No Network, No GUI)
lion run -- node script.js
lion run -- cargo build

# Allow internet access (e.g., for downloading packages)
lion run --network -- curl https://example.com

# Allow GUI rendering (Exposes X11/Wayland and fonts)
lion run --gui -- firefox

# Debug Run: See what bwrap commands will be executed without actually running them
lion run --dry-run -- ls -la
```

## Shortcomings & Limitations

Because the tool relies on hardcoded paths, it trades compatibility for simplicity:

- **Bubblewrap Dependency**: Requires `bwrap` to be installed on the host system.
- **Limited Compatibility**: Hardcoding `/usr` and `/lib` works for Ubuntu/Debian, but will drastically fail on heavily customized distros (e.g., NixOS, Arch subsets) or when symlinks are deeply nested.
- **Linux Only**: Fundamentally tied to Linux namespaces.
- **Rootless Limitations**: Only works in rootless setups; deeply nested system administration tasks cannot be sandboxed correctly.
- **No Resource Limits**: Currently does not restrict CPU or RAM usage.

## Next Up: The Scanner Module

Currently, the engine just blindly assumes `/usr` and `/bin` exist and are sufficient.

**Why we need a scanner back:** To make L.I.O.N portable across diverse Linux ecosystems, we plan to re-introduce a lightweight dynamic `scanner.rs`. Instead of assuming directories, a scanner checks the host OS to discover exact symlinks, library caches, and required D-Bus sockets, mapping them perfectly into the sandbox before execution. This is necessary to support more complex IDEs, language servers, and varying OS structures without breaking the sandbox.
