# How to Use L.I.O.N

L.I.O.N is designed to be a low-friction tool for running risky commands. This guide covers everything the tool can do and how to use it effectively.

## Core Commands

### Running a Sandbox

The primary command is `lion run`. Anything after the `--` separator is executed inside the sandbox.

```bash
lion run -- <command> <args>
```

#### Common Flags

- `--tui`: Launches the interactive dashboard (recommended).
- `--net=<mode>`: Sets the network policy. Modes: `none` (default), `allow`, `full`.
- `--clearenv`: Wipes all host environment variables (active by default).
- `--ro <path>`: Mounts an additional host path as read-only.
- `--dry-run`: Shows the final `bwrap` command without executing it.

### System Setup

On some systems (like Ubuntu 24.04+), unprivileged user namespaces are restricted. Use the install command once to set up the necessary profiles:

```bash
sudo lion install
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

### Web Development with Full Network

Run a dev server that needs internet access but keep your project source protected.

```bash
lion run --net=full --tui -- npm run dev
```

### Testing an Untrusted Script

Inspect every file a script tries to read without giving it access to your home directory.

```bash
lion run --tui -- bash ./untrusted_script.sh
```

### Dependency Installation

Install packages while only allowing access to specific official repositories.

```bash
lion run --net=allow --tui -- pip install -r requirements.txt
```

---

## Best Practices

1. **Always use --tui**: The visualization helps you catch unexpected behavior immediately.
2. **Keep src_access = "ro"**: This is one of L.I.O.N's strongest features for preventing accidental source tampering.
3. **Use --net=allow sparingly**: Only whitelist the domains you absolutely need for the specific command.
4. **Prefer --ro for extra mounts**: Avoid giving write access to host directories unless strictly required for the tool's function.
