# How to Use L.I.O.N

L.I.O.N is designed to be a low-friction tool for running risky commands. This guide covers everything the tool can do and how to use it effectively.

## Getting Started & Installation

Before running L.I.O.N, ensure you have the required dependencies installed on your system.

### Prerequisites

1. **Rust & Cargo**: Required to compile and build the L.I.O.N binary.
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **bubblewrap (`bwrap`)**: The underlying Linux sandboxing utility.
   ```bash
   # Ubuntu / Debian
   sudo apt install bubblewrap

   # Fedora
   sudo dnf install bubblewrap

   # Arch Linux
   sudo pacman -S bubblewrap
   ```

### Installation Modes

You can run L.I.O.N either directly in development mode (using Cargo) or by installing it system-wide.

#### Method A: Local Cargo Development (Ad-Hoc)
Use this if you want to test code changes or run commands without installing anything globally:
```bash
git clone https://github.com/A56-A5/lion.git
cd lion
cargo build
```

#### Method B: Global Installation
To install the binary globally into your `~/.cargo/bin` directory (ensure this path is added to your environment's `$PATH`):
```bash
cargo install --path .
```

## Core Commands


### Running a Sandbox

The sandbox can be executed either using the globally installed binary (`lion`) or directly via `cargo run` from the project repository. Anything after the `--` separator is executed inside the sandbox.

#### Global Installation
```bash
lion run [flags] -- <command> <args>
```

#### Local cargo run
```bash
cargo run -- run [flags] -- <command> <args>
```

#### Common Flags

- `--tui`: Launches the interactive dashboard (recommended).
- `--net=<mode>`: Sets the network policy. Modes: `none` (default), `allow`, `full`.
- `--clearenv`: Wipes all host environment variables (active by default).
- `--ro <path>`: Mounts an additional host path as read-only.
- `--dry-run`: Shows the final `bwrap` command without executing it.

### System Setup

On some systems (like Ubuntu 24.04+), unprivileged user namespaces are restricted by default. Run the install command once with root privileges to write and load a targeted AppArmor profile (permitting only `bwrap` to create user namespaces):

**For global installations:**
```bash
sudo $(which lion) install
```

**For local cargo builds:**
```bash
# Build the binary first
cargo build
# Run the installation from the build target
sudo ./target/debug/lion install
```


---

## The Interactive Dashboard (TUI)

Using the `--tui` flag provides a real-time view of the sandboxed process.

### Panels

- **Access Log**: Displays filesystem events.
    - READ: Successful read attempt.
    - WRITE: Successful write attempt.
    - BLOCKED: A permission denied event detected by the sandbox.
- **Process Tree**: Shows all child processes running inside the cage.
- **Modules / Paths**: Lists what parts of your host system are currently exposed.
- **Command Output**: The raw stdout/stderr of your program.
- **Performance**: Sparklines for CPU and Memory usage.

### Keyboard Shortcuts

- `Q`: Exit and kill the sandbox.
- `F`: Toggle auto-follow for the Access Log.
- `O`: Toggle auto-follow for the Command Output.
- `PgUp / PgDn`: Scroll through Command Output.
- `Up / Down`: Scroll through Access Log.

---

## Configuration

L.I.O.N uses TOML files for persistent configuration.

### Project Config (`lion.toml`)

Place a `lion.toml` in your project root to define default behavior for that project.

```toml
[sandbox]
project_access = "rw"    # Workspace access level
src_access = "ro"        # Protection for source files

[[mount]]
path = "~/tools/sdk"
access = "ro"
```

### Network Allow-list (`proxy.toml`)

When using `--net=allow`, L.I.O.N filters HTTP/HTTPS traffic through an internal proxy.

```toml
domains = [
  "github.com",
  "npmjs.org",
  "pypi.org"
]
```

### Global Config

Configuration at `~/.config/lion/lion.toml` applies to every run across your system.

---

## Practical Examples

Here are common developer scenarios showing how to invoke the sandbox. Both the globally installed CLI (`lion`) and local development CLI (`cargo run -- run`) variations are provided.

### 1. Web Development (Full Network Access)
Start a local Node/Python dev server with full internet access while keeping your private home directory credentials (like SSH keys, git configs) hidden.
* **Global install:**
  ```bash
  lion run --net=full --tui -- npm run dev
  ```
* **Local development:**
  ```bash
  cargo run -- run --net=full --tui -- npm run dev
  ```

### 2. Dependency Installation (Domain Whitelist Network)
Install Python/Node modules while whitelisting only the official package registries to mitigate dependency resolution hijacking.
* **Global install:**
  ```bash
  lion run --net=allow --tui -- pip install -r requirements.txt
  ```
* **Local development:**
  ```bash
  cargo run -- run --net=allow --tui -- pip install -r requirements.txt
  ```

### 3. Testing an Untrusted Script (No Network, High Isolation)
Run custom bash/shell scripts in a network-isolated environment to observe every filesystem access attempt live.
* **Global install:**
  ```bash
  lion run --tui -- bash ./untrusted_script.sh
  ```
* **Local development:**
  ```bash
  cargo run -- run --tui -- bash ./untrusted_script.sh
  ```

### 4. Running GUI Applications Safely (e.g., Google Chrome)
Isolate web browser processes from your host cookies, saved logins, and profile history while forwarding graphics sockets to display the browser window.
* **Global install:**
  ```bash
  lion run --gui --net=full --tui -- google-chrome
  ```
* **Local development:**
  ```bash
  cargo run -- run --gui --net=full --tui -- google-chrome
  ```

### 5. Shielding Against Malicious Build Scripts (`build.rs`)
In Rust, dependencies can execute arbitrary code on compilation via build scripts (`build.rs`). Run compilation inside L.I.O.N without network access to prevent credentials extraction.
* **Global install:**
  ```bash
  lion run --net=none --tui -- cargo build
  ```
* **Local development:**
  ```bash
  cargo run -- run --net=none --tui -- cargo build
  ```

### 6. Sandbox Dry-Run (Verification)
Check the exact commands, namespaces, environment overrides, and volume binds that L.I.O.N is generating for bubblewrap (`bwrap`) without actually spawning a container.
* **Global install:**
  ```bash
  lion run --dry-run -- ls -la
  ```
* **Local development:**
  ```bash
  cargo run -- run --dry-run -- ls -la
  ```


---

## Best Practices

1. **Always use --tui**: The visualization helps you catch unexpected behavior immediately.
2. **Keep src_access = "ro"**: This is one of L.I.O.N's strongest features for preventing accidental source tampering.
3. **Use --net=allow sparingly**: Only whitelist the domains you absolutely need for the specific command.
4. **Prefer --ro for extra mounts**: Avoid giving write access to host directories unless strictly required for the tool's function.
