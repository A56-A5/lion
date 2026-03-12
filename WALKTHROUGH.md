# Walkthrough - L.I.O.N (Profile Branch)

This document provides a technical walkthrough of the features implemented on the `profile` branch, covering Tasks 1, 2, and 3 of the roadmap.

## Task 1: Harden the Sandbox Core [DONE]
The foundation of L.I.O.N has been hardened with improved isolation flags and a structured root filesystem.
- **Isolation Flags**: Added `--die-with-parent`, `--hostname lion`, and `--new-session` to ensure the sandbox is self-terminating, identifiable, and detached from the host terminal.
- **Structured Root**: Uses a clean `--tmpfs /` with explicit `--dir` stubs for `/usr`, `/bin`, `/lib`, `/tmp`, and `/run`.

## Task 2: Exposure Control [DONE]
Implemented a single, persistent profile system that allows users to manage sandbox permissions without complex configuration files.
- **Profile Store**: Located at `~/.config/lion/profile.json`.
- **CLI Commands**:
  - `lion status`: View current exposure levels.
  - `lion expose`: Add modules (network, gpu, etc.) or custom writable host paths.
  - `lion unexpose`: Remove permissions.
- **Security Validation**: Custom paths are strictly validated to prevent exposing sensitive system directories like `/etc` or `/root`.

## Task 3: Module Resolver [DONE]
The resolver handles the translation of high-level profile settings into concrete `bwrap` arguments.
- **Mandatory Base**: Always ensures core system paths are included.
- **Dynamic Resolution**: Merges system modules with user-defined paths, performing existence checks to ensure the host paths actually exist before attempting to mount them.

## Additional Hardening: Environment Sanitization
- **Strict Isolation**: All host environment variables are now cleared using `--clearenv`.
- **Safe Allowlist**: Only a minimal set of strictly safe variables (`HOME`, `USER`, `PATH`, etc.) is injected into the sandbox, preventing secret leakage.

## CLI Ergonomics
- **Shorthand Flags**: Support for `--network`, `--gpu`, `--wayland`, `--x11`, and `--audio` has been added directly to the `expose` and `unexpose` commands for a smoother user experience.

---
Verified locally on the `profile` branch.
