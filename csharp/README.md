# Pancake (C# rewrite)

This is the in-progress C# rewrite of Pancake, described and tracked in
`../readmenow.md`. That file is the narrative log — decisions, what got
verified and how, known gaps. **This file is the map**: what each
project is, how to build it, how to run the tests, and where to start
reading if you're picking this up cold.

## Layout

Eight projects, each mirroring a piece of the original Rust source
(`../src/`):

| Project | Rust equivalent | What it is |
|---|---|---|
| `Pancake.Syrup` | `src/syrup/` | The `.confi` config format — native, C-style, and Lua frontends. Complete port. |
| `Pancake.Config` | `src/config.rs` | Runtime config loading (`.confi`/legacy `.toml`) + SIGHUP reload. Complete port. |
| `Pancake.Cn` | `src/backend/gpu.rs`, `src/backend/udev.rs` (the low-level slice) | Raw P/Invoke against `libdrm`/`libgbm`/`libEGL`/`libinput`/`libc`. This is what "Cn" turned out to mean — see below. |
| `Pancake.Shell` | `src/shell/` + the orchestration half of `src/state.rs` | `Rectangle`/`Point`/`Size` geometry, the BSP tiling tree (`TileTree<TWindow>`), `PancakeSpace` (window/output container), `WorkspaceManager`, `PancakeCompositorState` (retile/cycle_focus/snap_focused/etc). |
| `Pancake.Render` | `src/render/` | The Aero blur pipeline (`AeroRenderer`, via `Silk.NET.OpenGLES`), window borders, title-bar decorations + click hit-testing, xcursor loading. |
| `Pancake.Wayland` | `src/handlers/` (minus `input.rs`/`xwayland.rs`) + `src/state.rs`'s protocol delegation | A real `NWayland.Server`-backed Wayland compositor: `wl_compositor`, `xdg_shell`, `xdg_decoration`, `wlr_layer_shell`, `wl_seat`, `wl_data_device_manager`, and the real frame loop tying `Pancake.Cn` + `Pancake.Render` together. |
| `Pancake.App` | `src/main.rs` + `src/backend/winit.rs` | The actual entry point: CLI args, SIGHUP install, backend dispatch (`--winit` for a nested dev window, default for the udev/DRM path). |
| `Pancake.Syrup.Smoke` | — (no Rust equivalent) | Not a real project — the test harness. Every claim in `readmenow.md` about something being "verified" was checked here. Run this first when picking the project back up, to see what's actually working right now in your environment. |

### What "Cn" turned out to be

The original plan was a small custom low-level language ("Cn") for the
raw-pointer/FFI parts C# can't express cleanly. Turned out there was no
real gap: `libdrm`/`libgbm`/`libEGL`/`libinput` are all just ordinary
shared libraries with stable C APIs, so plain C# `[LibraryImport]`
P/Invoke says everything needed. **`Cn` is now just the name of the
project holding that P/Invoke code**, not a separate language — see
`readmenow.md`'s "Cn collapses to a naming convention" section for the
full reasoning.

## Building

```sh
cd csharp
dotnet build
```

If you hit an SDK workload-resolver error (`Workload set version ...
has missing manifests`), that's a broken local `dotnet` install
unrelated to this project — work around it without needing root:

```sh
MSBuildEnableWorkloadResolver=false dotnet build
```

(A permanent fix needs `dotnet workload repair` with elevated
privileges; the env var above is the no-privileges workaround used
throughout this project's development.)

## Running the tests

```sh
MSBuildEnableWorkloadResolver=false dotnet run --project Pancake.Syrup.Smoke
```

This isn't a unit-test framework — it's a single console program that
runs ~145 real checks end to end (real files, real sockets, real
hardware devices where available) and prints `OK`/`FAIL` per check,
ending with a pass/fail summary. A few checks depend on things your
environment may or may not have (a real DRM render node, a real
`/dev/input` device, an X11/Wayland cursor theme) and print `SKIP`
instead of failing when those aren't present. Read the top of
`Pancake.Syrup.Smoke/Program.cs` and `RawWaylandClient.cs` for what
each section actually checks and why.

## Running the app itself

```sh
MSBuildEnableWorkloadResolver=false dotnet run --project Pancake.App -- --winit   # nested dev window
MSBuildEnableWorkloadResolver=false dotnet run --project Pancake.App             # udev/DRM backend
```

Neither currently reaches real on-screen pixels — see "Status" below.

## Status (short version — `readmenow.md` has the full story)

- Every protocol Pancake's Rust `state.rs` advertises has a real,
  wire-verified C# implementation.
- The frame loop is real: a live GPU context (when available) drives
  `AeroRenderer`, and clients get real `wl_callback.done` frame
  callbacks either way.
- **Not built yet:** real DRM atomic modeset/pageflip (turning a
  rendered frame into actual on-screen pixels — only enumeration
  exists), real hardware input event delivery (`input.rs`'s own logic
  is ported, but nothing drives it — see the libinput note below), and
  XWayland (a different protocol entirely, no library chosen yet).
- Two sandbox-specific bugs were found and handled properly during
  development, not papered over: a `libinput_dispatch()` segfault
  (isolated into a subprocess so it can't take down the test run) and a
  GLFW GL-context-creation hang (bounded with a watchdog). Both are
  environment issues in the sandbox this was built in, confirmed via
  reproduction outside .NET — they may or may not reproduce on your
  machine.
