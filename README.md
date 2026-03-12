# L.I.O.N

A lightweight, per-execution filesystem sandbox for Linux using Bubblewrap (`bwrap`).

## Features & Maximum Capability

L.I.O.N currently operates as a static, pre-hardcoded sandbox engine. It protects your development environment by isolating processes from the rest of your system.

- **Maximum Sandboxing**: Creates an entirely new Linux namespace where your command runs. It isolates user IDs, inter-process communication, Process Trees (PIDs), hostnames, and cgroups. 
- **Hardcoded System Mounts**: Exposes only basic standard paths read-only to guarantee executables work (`/usr`, `/bin`, `/lib`, `/lib64`, `/etc/alternatives`, `/snap`).
- **Source Protection**: Re-mounts your current working directory as read-write, but strictly restricts the `src/` folder to be read-only so tests cannot maliciously overwrite your code.
- **Environment Isolation**: Passes only safe environment variables (`HOME`, `USER`, `PATH`, `LANG`, and basic `XDG_*` vars).
- **Selective Capabilities**: You can opt-in to expose network interfaces (`--network`) or GUI rendering sockets (`--gui`).

For a detailed breakdown of EXACTLY what this sandbox exposes to applications, see [EXPOSURES.md](file:///home/vishnunandan555/Projects/lion/EXPOSURES.md).

## System Setup

L.I.O.N requires `bwrap` to be installed on your system.
Additionally, modern Linux distributions (like Ubuntu 24+) restrict unprivileged user namespaces via AppArmor. L.I.O.N provides an installation command to configure this automatically without sacrificing global security:

```bash
# Creates a targeted AppArmor profile specifically for bwrap
sudo lion install
```

## Usage Guide

Execute any command safely isolated from your home directory:

```bash
# Basic sandboxed execution (No Network, No GUI)
lion run -- node script.js
lion run -- cargo build

# Allow internet access (e.g., for downloading packages or resolving DNS)
lion run --network -- curl https://example.com

# Allow GUI rendering (Exposes X11/Wayland sockets and fonts)
lion run --gui -- firefox

# Debug Run: See what bwrap command will be executed without actually running it
lion run --dry-run -- ls -la

# The --optional flag exists but does nothing in the current build
lion run --optional audio -- ls
```

## Shortcomings & Limitations

Because the tool relies on monolithic hardcoded paths, it trades compatibility for simplicity:

- **Bubblewrap Dependency**: Requires `bwrap` to be installed on the host system.
- **Limited Compatibility**: Hardcoding `/usr` and `/lib` works for Ubuntu/Debian, but will fail on heavily customized distros (e.g., NixOS, Arch subsets) or when symlinks are deeply nested.
- **GUI Support**: GUI sandboxing requires GPU hardware access (`/dev/dri`) and session sockets. While most native apps work, some complex D-Bus services or global shortcuts may be restricted.
- **Linux Only**: Fundamentally tied to Linux namespaces.
- **Snap Packages are Incompatible**: Ubuntu's default `firefox` and other apps installed via `snap` **will not work**. Snap packages rely on `snap-confine`, which requires root-level capabilities (like `CAP_MAC_ADMIN`). Because L.I.O.N creates rootless, unprivileged user namespaces (`bwrap --unshare-user`), the Linux kernel explicitly strips these capabilities, making it technically impossible to run Snaps inside this sandbox.

### How to run Firefox in L.I.O.N:

To run Firefox securely inside the sandbox, you must use the native binary version instead of the Snap version.

1.  **Download Firefox**: Get the Linux 64-bit `.tar.bz2` from [Mozilla.org](https://www.mozilla.org/en-US/firefox/all/#product-desktop-release).
2.  **Extract**: `tar xfj firefox-*.tar.bz2`
3.  **Run with L.I.O.N**:
    ```bash
    # Ensure you are inside the extracted firefox directory
    lion run --network --gui -- ./firefox
    ```

- **Rootless Limitations**: Only works in rootless setups; deeply nested system administration tasks cannot be sandboxed correctly.
- **No Resource Limits**: Currently does not restrict CPU or RAM usage limit.

## Architecture

L.I.O.N is built with a modular engine located in `src/sandbox_engine/`:

- `builder.rs`: Configures bubblewrap namespaces.
- `environment.rs`: Sanitizes environment variables.
- `mounts.rs`: Handles all bind-mount logic.
- `runner.rs`: Orchestrates the execution flow.
- `userns.rs`: Pre-flight checks for User Namespaces.

## Roadmap

### 1. Granular Networking Profiles
We are moving away from the simple `--network` boolean toggles towards protocol-aware profiles:
- `none`: Default isolation.
- `dns`: Only allow UDP/TCP port 53.
- `http`: Restrict access to ports 80/443 via a user-space proxy (slirp4netns + internal broker).
- `full`: Complete host network sharing.

### 2. The Scanner Module
To make L.I.O.N portable across diverse Linux ecosystems, we plan to construct a lightweight dynamic `scanner.rs`. Instead of assuming directories, a scanner checks the host OS to discover exact symlinks, library caches, and required D-Bus/DRI sockets, mapping them perfectly into the sandbox before execution.
