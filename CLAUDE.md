# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

ALVR streams VR games from a PC (the *streamer/server*) to a standalone VR headset (the *client*) over Wi-Fi. PC side runs as a SteamVR driver; client side runs on Android headsets (Quest, Pico, Vive Focus, etc.) via OpenXR, plus a separate Apple Vision Pro app (not in this repo).

The repo is a Cargo workspace (`alvr/*`). All builds, formatting, packaging, and dependency prep go through the **xtask** crate — there is no `make` or top-level build script.

## Build & dev commands

Use `cargo xtask` for everything beyond `cargo check`/`clippy`/`test`. Run from the workspace root.

| Task | Command |
|---|---|
| Install external deps (FFmpeg, OpenXR loaders, etc.) | `cargo xtask prepare-deps --platform {windows\|linux\|macos\|android}` (add `--no-nvidia` on Linux without NVIDIA) |
| Build streamer (PC side, SteamVR driver + dashboard) | `cargo xtask build-streamer --platform {windows\|linux}` |
| Build + run streamer (opens dashboard) | `cargo xtask run-streamer` (add `--no-rebuild` to skip rebuild) |
| Build launcher | `cargo xtask build-launcher --platform {windows\|linux}` / `run-launcher` |
| Build Android client APK | `cd alvr/client_openxr && cargo xtask build-client` |
| Build C-ABI libs | `build-client-lib`, `build-client-xr-lib`, `build-server-lib` (each runs `cbindgen` to regenerate the matching C header alongside the static lib) |
| Package (distribution profile + archive) | `package-streamer`, `package-launcher`, `package-client`, `package-client-lib` |
| Format | `cargo xtask format` / check only: `cargo xtask check-format` |
| Lint (project clippy lint set) | `cargo xtask clippy` (CI uses `--ci`) |
| Tests | `cargo test -p alvr_session` — this is the only test target CI runs. Most crates have no tests. |
| Bump version | `cargo xtask bump --version <ver>` (add `--nightly` for nightly tag) |
| Clean (build/, deps/, target/) | `cargo xtask clean` |

Useful flags: `--release` (optimized debug build), `--profiling` (enable profiling), `--gpl` (bundle FFmpeg, Windows only), `--keep-config` (preserve `session.json` across rebuilds), `--meta-store`/`--pico-store` for store packaging.

Build artifacts land in `./build/`; downloaded/compiled deps in `./deps/`.

## Architecture

### Streamer ↔ client split

The system is split across a process boundary that ships compressed video, audio, tracking, and control data over the network:

- **Streamer (PC)** — `alvr/server_core` is the platform-independent core (encoding, networking, session state, bitrate adaptation). `alvr/server_openvr` wraps it as a SteamVR driver (C++ bindings generated in `build.rs`; the `bindings` module is included from `OUT_DIR`). `alvr/dashboard` is the GUI (egui) that talks to the driver. `alvr/launcher` installs/updates the streamer.
- **Client (headset)** — `alvr/client_core` is the platform-independent client (decoding, tracking submission, sockets). `alvr/client_openxr` is the Android OpenXR entry point that drives it. Decoder backends live in `client_core/src/video_decoder/` (MediaCodec on Android).
- **Wire protocol** — `alvr/sockets` (control + stream sockets), `alvr/packets` (message types). The control socket runs a lifecycle handshake; the stream socket carries video/audio/haptics/tracking at low latency.

### Shared crates (workspace-wide)

- `alvr_common` — re-exports core deps (`anyhow`, `glam`, `log`, `parking_lot`, `semver`, `settings_schema`), defines `Pose`, `Fov`, `ViewParams`, ID constants (`HEAD_ID`, `HAND_LEFT_ID`, …), `ConnectionState`, `LifecycleState`, logging macros.
- `alvr_session` — the **settings schema**. The single source of truth for what's configurable; the dashboard UI is generated from it. The only crate with meaningful unit tests in CI.
- `alvr_server_io` / `alvr_filesystem` — session.json persistence and the FHS-aware path layout (`Layout`, `build_dir()`, `streamer_build_dir()`, etc. — use these rather than hardcoding paths).
- `alvr_graphics` — wgpu-based rendering helpers shared by client and server (compositor, lobby).
- `alvr_audio`, `alvr_events`, `alvr_system_info`, `alvr_adb`, `alvr_gui_common` — focused utility crates.

### Native/FFI surfaces

Several crates expose C ABIs (`c_api.rs` + `cbindgen.toml`) so the Rust core can be embedded in C++ (`server_openvr` SteamVR driver, third-party clients like the Apple Vision Pro app). When changing a `c_api.rs`, rerun the matching `cargo xtask build-*-lib` so the regenerated `.h` ships alongside the lib — downstream C/Swift consumers track the header, not the Rust source. `server_openvr/build.rs` generates Rust bindings from the C++ side via bindgen. `vrcompositor_wrapper` and `vulkan_layer` are Linux-specific shims around SteamVR's compositor.

### The xtask crate

`alvr/xtask` is *not* a library — it's the build system. Subcommands are dispatched from `main.rs` (see `HELP_STR`). `build.rs`, `dependencies.rs`, `packaging.rs`, `format.rs`, `ci.rs`, `version.rs` each own a subset of commands. When adding a new build step, extend xtask rather than introducing shell scripts.

## Conventions

`CONTRIBUTING.md` documents the project's Rust style rules — read it before significant changes. Highlights that aren't standard Rust:

- `unwrap()` and `panic!()` are discouraged; bubble errors instead. Prefer `unreachable!()` for impossible match arms. Prefer `.get()` over `[]` indexing. Add a `// # Safety` comment justifying any retained `unwrap()` or raw index.
- Use `maybe_` prefix for `Option`/`Result` *locals* (never for parameters/fields), and `_dir`/`_path`/`_fname` suffixes when both directories and files appear in the same scope.
- Model invalid states out of existence with enums rather than redundant booleans (e.g. `enum State { Paused, Resumed, Streaming }` instead of `resumed: bool, streaming: bool`).
- File top-level ordering: private imports → public imports → ffi bindings → private consts → public consts → private structs → public structs → private fns → public fns.
- Extract constants for any "arbitrary" literal (timeouts, intervals) at the top of the file, using rich types (`Duration`, `Path`) where possible.

Rust edition 2024, MSRV 1.92 (see workspace `Cargo.toml`). The `distribution` profile (release + LTO) is used for shipped artifacts.
