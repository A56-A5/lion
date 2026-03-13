# L.I.O.N 🦁

> **L**inux **I**solation & **O**bservability **N**etwork.
> Hardened, real-time sandboxing for untrusted commands.

L.I.O.N is a security-first sandbox engine for Linux built on top of `bubblewrap` (`bwrap`). It allows you to run any CLI tool, package manager, or script in a "cage" that is physically isolated from your home directory, your credentials, and your system secrets.

What makes L.I.O.N unique is **Observability**: it doesn't just block access; it shows you exactly what the program is trying to do in real-time.

---

## ⚡ Key Features

- **Extreme Isolation**: Every run starts with a blank synthetic root (`/`). Only essential system paths are bind-mounted.
- **Environment Scrubbing**: Automatically wipes sensitive environment variables (AWS keys, GitHub tokens, etc.) before execution.
- **Live Observability**: Real-time tracking of file access (Read/Write/Delete) and blocked permission attempts.
- **Performance Monitoring**: Visual CPU and RAM sparklines for the sandboxed process tree.
- **Command Output Mirroring**: Captures and displays raw command output inside a dedicated TUI panel.
- **Network Control**: Choose between `None` (fully isolated), `Allow` (domain allow-list), or `Full`.
- **Source Protection**: Automatically re-mounts your project's `src/` directory as read-only, even if the project root is writable.
- **Dependency Management**: Built-in support for common tools like `npm`, `cargo`, `pip`, and `git`.

---

## 🚀 Installation

### 1. Prerequisites
Ensure you have `bubblewrap` installed on your host system:
```bash
# Ubuntu/Debian
sudo apt install bubblewrap

# Fedora
sudo dnf install bubblewrap

# Arch
sudo pacman -S bubblewrap
```

### 2. Build & Install
Clone the repository and install using Cargo:
```bash
git clone https://github.com/A56-A5/lion.git
cd lion
cargo install --path .
```

### 3. AppArmor Setup (Required for Ubuntu 24.04+)
Modern Linux distributions restrict unprivileged user namespaces. Run the targeted installer once to set up the necessary AppArmor profile:
```bash
sudo lion install
```

---

## 🖥️ The L.I.O.N Dashboard

Run any command with the `--tui` flag to enter the observability dashboard:

```bash
lion run --tui -- npm run dev
```

### Dashboard Panels
- **ACCESS LOG**: Live stream of filesystem events (✓ READ, ✏️ WRITE, ⚠️ BLOCKED).
- **PROCESS TREE**: Real-time view of all child processes running inside the cage.
- **MODULES / PATHS**: Lists active security modules and every host path exposed to the sandbox.
- **COMMAND OUTPUT**: Mirror of the program's raw stdout/stderr (progress bars, logs, etc.).
- **PERFORMANCE**: CPU and RAM usage sparklines.

### Key Bindings
- `Q`: Exit sandbox (kills all processes).
- `F`: Toggle auto-follow for the Access Log.
- `O`: Toggle auto-follow for the Command Output.
- `PgUp / PgDn`: Scroll the Command Output panel.
- `↑ / ↓`: Scroll the Access Log.

---

## ⚙️ Configuration

L.I.O.N uses a hierarchical configuration system. Settings are merged in this order:
1. **Built-in Defaults**: Safe defaults for common developer tools.
2. **Global Config** (`~/.config/lion/lion.toml`): Persistent settings for all projects (e.g., exposing your `~/flutter` SDK).
3. **Project Config** (`./lion.toml`): Overrides and extra mounts for a specific project.

### Project Example (`lion.toml`)
```toml
[sandbox]
project_access = "rw"    # Allow writes to the project root
src_access = "ro"        # But keep src/ protected

[[mount]]
path = "~/datasets"      # Expose a specific host directory
access = "ro"
```

### Network Allow-list (`proxy.toml`)
When using `--net=allow`, L.I.O.N uses an embedded proxy to filter traffic. You can define allowed domains:
```toml
domains = [
  "api.myapp.com",
  "auth.provider.io"
]
```

---

## 🛠️ Usage Examples

**Web Development (with Live Reload)**
```bash
lion run --net=full -- npm run dev
```

**Network Isolation (Testing a Native Browser)**
```bash
# Browser launches but cannot reach anything (Network blocked)
lion run -- microsoft-edge

# Safe browsing: only domains in proxy.toml are reachable
lion run --net=allow -- microsoft-edge
```

**Filesystem Observability (Monitoring an Editor)**
```bash
# See every system file and config the editor tries to read in real-time
lion run --tui -- gnome-text-editor
```

**Checking Exposures (File Manager)**
```bash
# Verify what folders are visible before actually running
lion run --dry-run -- nautilus
```

---

## 🛡️ Security Philosophy

L.I.O.N is built on the **Principle of Least Privilege**. If a program doesn't explicitly need it, it doesn't see it. It shields your `~/.ssh`, `~/.gnupg`, browser cookies, and shell history from every command you run.

For a detailed technical breakdown of what is isolated, see **[EXPOSURES.md](EXPOSURES.md)**.

---

## 🤝 Contributing

Contributions are welcome! Please see the architecture breakdown in the source code to get started.

---

## 📜 License
MIT
