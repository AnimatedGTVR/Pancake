# Pancake Rewrite Plan

## Why

Pancake is currently written in Rust on top of Smithay. The problem: I don't
know Rust. I can read it well enough to follow along, but I'm not able to
actually write/extend it comfortably, which makes this my own project I
can't really work on.

I *can* read C# and write a little of it. So the plan is to rewrite Pancake
in C#, so I can actually understand and work on my own compositor.

This file exists so I don't lose the plan between sessions. Update it as
decisions get made — it's a living doc, not a spec that's final on day one.

## The split: C# + Cn

Most of a Wayland compositor is not low-level — window state, the shell,
input handling, layout, config, blur/theme logic. That's all fine in normal
C#.

The genuinely ugly part is the small slice that needs to do things C# proper
doesn't do cleanly: raw pointers, syscalls, DRM/KMS ioctls, FFI boundaries,
that kind of thing. Rather than dropping into C's rules for the whole
project (or fighting C#'s safety rails for just a few files), that slice
gets its own small language: **Cn**.

Cn is not a general-purpose language. It's not trying to be a full C# or a
full C. It only needs to do exactly what the ugly bits need and nothing
else — think of it as C#-flavored syntax wrapped around the handful of
low-level operations the compositor actually requires (raw memory access,
syscalls/ioctls, calling into C libraries). Everything else stays in
regular C#.

Where Cn actually needs to land (interop mechanism, how it compiles down,
whether it's a source-to-source layer over `unsafe` C# or something
lower) is not decided yet. That's an open design question, not settled.

## What currently exists (Rust, being replaced)

Current Rust source is ~4500 lines across:

- `src/handlers/` — Wayland protocol handlers
- `src/backend/` — GPU/DRM/KMS backend (`gpu.rs`)
- `src/render/` — rendering, including the Aero blur shader work (`aero.rs`)
- `src/shell/` — window shell logic
- `src/syrup/` — (whatever this is doing today — check before porting)
- `src/config.rs` — config loading

`unsafe` is only actually used in three files today:
`src/backend/gpu.rs`, `src/config.rs`, `src/render/aero.rs`. That's a decent
first guess at the real Cn surface — everything else is likely safe,
ordinary logic that ports to plain C# without needing Cn at all.

Existing feature status (from the old README, still true, just needs a new
implementation):

- XDG-shell application windows
- Window mapping, raising, maximize, fullscreen, close
- Popup tracking
- Keyboard and pointer input, click-to-focus
- `Super+Tab` window cycling
- XWayland support
- Basic cascading window placement
- Winit backend (nested testing) + DRM/KMS backend (real hardware)
- Early Aero blur shader work

None of this is being redesigned feature-wise — the goal is a rewrite in a
language I can actually work in, not a redesign of what Pancake does.

## Decided

- **Cn compiles source-to-source into plain C# `unsafe` blocks**, then
  gets built normally by the regular C# compiler. Keeps the toolchain
  simple and means the generated output is always readable/debuggable
  ordinary C#, not a mystery binary from a from-scratch compiler.
- **DRM/KMS backend**: searched for an existing C# binding library first,
  per the fallback plan below. Found two candidates, both dead:
  [DRI.net](https://github.com/jpbruyere/DRI.net) (DRM/KMS/GBM/EGL
  bindings, last pushed 2017, 3 stars) and
  [WaylandSharp](https://github.com/KeKl/WaylandSharp) (Wayland protocol
  bindings, last pushed 2016, 1 star, effectively empty). Neither is
  usable as-is. **Conclusion: hand-write the DRM/KMS layer in Cn** —
  nothing worth building on top of exists.

- **`src/syrup/`**: this is Pancake's `.confi` config-file system, not a
  low-level concern at all. It's a "universal config IR" — three small
  frontends (native Syrup block syntax, Lua 5.4, and a C++/C#-style
  syntax that reuses the native parser and just strips the leading type
  keyword) that all parse down to one common `SyrupDoc` (nested
  string→typed-value map) the compositor reads from. 460 lines total
  (`mod.rs` 169, `lua.rs` 114, `native.rs` 177). No `unsafe` anywhere —
  the native/C-style parser is plain line-based string parsing, and the
  Lua frontend goes through `mlua` (its FFI is hidden inside that crate's
  own safe wrapper, never exposed to Pancake's code). **Ports straight to
  C#, no Cn involved.** Lua support has a direct equivalent:
  [NLua](https://github.com/NLua/NLua), the standard actively-maintained
  C# Lua binding, same shape as `mlua` — safe managed API over FFI it
  owns internally.

- **C# Wayland server library — investigated, `NWayland` wins.** Compared
  four candidates:

  | Library | Server-side? | Status | Verdict |
  |---|---|---|---|
  | [**NWayland**](https://github.com/AvaloniaUI/NWayland) (`AvaloniaUI/NWayland`) | **Yes — 100% managed** | Active (pushed within the last ~6 weeks as of this check), 18★, MIT, maintained by the AvaloniaUI team | **Use this** |
  | [X9VoiD/WaylandSharp](https://github.com/X9VoiD/WaylandSharp) | No — client-only, README says so explicitly | Semi-active, 26★ | Ruled out — no server support |
  | [tadeokondrak/WaylandNET](https://github.com/tadeokondrak/WaylandNET) | No — "Wayland client" only | Stale (last pushed Feb 2024), 9★ | Ruled out — no server support |
  | [KeKl/WaylandSharp](https://github.com/KeKl/WaylandSharp) | Unknown, effectively empty repo | Dead (last pushed 2016), 1★ | Ruled out — dead |

  **NWayland covers the Wayland wire-protocol layer only** — both client
  and server. The server half is genuinely "100% managed": no
  `libwayland-server` C dependency at all (that library's C# usability
  path got deprecated/removed from Mesa a while back, which is exactly
  why NWayland reimplemented the server wire protocol in pure C#). Ships
  as separate NuGet packages: `NWayland` (core), `NWayland.Server`,
  plus optional `NWayland.Protocol.Plasma` and `NWayland.Protocol.Wlr`
  for KDE/wlroots-specific protocol extensions.

  **What it does *not* cover — still Cn/hand-written territory:** DRM/KMS,
  input (libinput/libseat), and GPU/graphics integration. NWayland solves
  "how do I speak the Wayland protocol as a server," not "how do I put
  pixels on a real screen" or "how do I read a real keyboard." That part
  of the plan is unchanged from the DRM/KMS decision above — still
  hand-written in Cn, since NWayland doesn't touch that layer at all.
  Whether `xdg-shell`/`xdg-decoration`/layer-shell are in the core package
  or need the `Wlr`/`Plasma` extension packages needs checking once
  actual porting starts — not confirmed yet.

- **What `src/handlers/`, `src/shell/`, `src/render/` actually lean on
  Smithay for** — grepped real usage across all three instead of guessing
  from the Cargo.toml feature list. Smithay is doing four genuinely
  different jobs that were easy to lump together as "one dependency":

  1. **Protocol glue** (`smithay::wayland::*`, `reexports::wayland_server`)
     — every file in `src/handlers/` (`compositor.rs`, `xdg_shell.rs`,
     `xdg_decoration.rs`, `layer_shell.rs`, `input.rs`) and `src/state.rs`
     import this. This is exactly the layer `NWayland` replaces — Smithay
     is providing typed handler traits over the same wire protocol
     `NWayland.Server` speaks directly. **No Cn needed here** — this is
     the port target for `NWayland`, confirmed.
  2. **Desktop-helper bookkeeping** (`smithay::desktop::Space`,
     `smithay::desktop::Window`) — used in `src/backend/gpu.rs` (space
     iteration for rendering) and `src/handlers/input.rs` (`surface_under`
     hit-testing, window-under-cursor lookup). This is *not* low-level —
     it's plain window-stacking/geometry logic (which window is where, in
     what order, what's under the cursor). NWayland doesn't provide an
     equivalent (it's wire-protocol only, no desktop-shell helpers), so
     this becomes **hand-written plain C#** — genuinely new code, but
     ordinary safe logic, not Cn territory.
  3. **GPU rendering** (`smithay::backend::renderer::gles::GlesRenderer`,
     `GlesFrame`, `GlesError`) — this is what `src/render/aero.rs` and
     `src/render/mod.rs` actually call, not raw GL/EGL calls directly.
     Smithay wraps GLES here; the ffi import in `aero.rs` is Smithay's own
     GLES ffi module, not something Pancake hand-rolled. A C# GL binding
     (e.g. Silk.NET or OpenTK, both active/mature) can plausibly replace
     the actual draw-call surface once a context exists — **but context
     creation/binding to a real DRM/KMS scanout buffer is still Cn**, this
     doesn't change the earlier DRM/KMS conclusion.
  4. **Backend/session/device layer** (`src/backend/udev.rs`,
     `src/backend/gpu.rs`, `src/backend/winit.rs`) — device enumeration,
     DRM/KMS, EGL, session/seat handoff (`backend_session_libseat`),
     libinput. This is the `backend_drm`/`backend_gbm`/`backend_egl`/
     `backend_udev`/`backend_session*`/`backend_libinput` feature set from
     Cargo.toml, confirmed by direct import in `udev.rs`. **This is the
     core Cn surface** — matches and sharpens the earlier DRM/KMS
     decision, now scoped concretely to these three files rather than a
     guess from `unsafe` usage alone.

  Net effect: the earlier "only 3 files use `unsafe`, that's the Cn
  surface" guess undercounted — `src/backend/udev.rs` and
  `src/backend/winit.rs` don't currently use `unsafe` in Rust (Smithay's
  own `unsafe` is hidden inside its safe API), but they're still doing
  work Smithay's C-facing backends handle for free that Cn will have to
  do explicitly once there's no Smithay underneath. The real split is:
  **`src/handlers/` → NWayland. `src/shell/` and the desktop-bookkeeping
  parts of `src/backend/`/`src/handlers/input.rs` → plain hand-written
  C#. `src/render/` draw calls → maybe a C# GL binding. `src/backend/udev.rs`
  + DRM/KMS/session/input plumbing → Cn.**

- **NWayland protocol coverage — resolved.** Checked the actual
  `NWayland.csproj` build file on GitHub: the core `NWayland` package
  generates its bindings from `external/wayland/protocol/wayland.xml`
  **plus all of `external/wayland-protocols`** (everything under
  `stable/`, `staging/`, and `unstable/` in that submodule) — which is
  exactly where `xdg-shell` and `xdg-decoration` live upstream.
  **`xdg-shell`/`xdg-decoration` are in the core `NWayland` package, no
  extension package needed.** Layer-shell (`wlr-layer-shell-unstable-v1`)
  comes from a *different* upstream repo (`wlr-protocols`, KDE/wlroots
  extensions, not part of `wayland-protocols`), and NWayland only
  generates that into the separate `NWayland.Protocols.Wlr` package
  (confirmed by its own submodule pointing at
  `gitlab.freedesktop.org/wlroots/wlr-protocols`). **So: core `NWayland`
  covers `xdg-shell`/`xdg-decoration`, `NWayland.Protocols.Wlr` is needed
  in addition for layer-shell.** Both are small NuGet references either
  way, not a blocker.

- **GLES draw calls vs. Cn — resolved, stays split.** Looked at whether a
  C# GL binding (Silk.NET, the obvious candidate) can cover
  `src/render/aero.rs`'s actual GLES calls. Two separate things are
  bundled in that file today: (a) creating an EGL context bound to a
  GBM/DRM buffer with no window system underneath, and (b) the actual
  draw calls (shader compile/link, blur passes, framebuffer ops) once a
  context exists. Silk.NET dropped its own EGL bindings in the 2.0
  transition and isn't bringing them back before 3.0 — headless
  EGL+GBM+DRM context setup isn't something it provides today. **Context
  creation stays Cn** (same device layer as the DRM/KMS decision above —
  it's really the same problem, not a separate one). But **once Cn hands
  back a live GL context, the actual draw calls can go through
  Silk.NET.OpenGLES** — Silk.NET's GL wrapper only needs a function-pointer
  loader (`GetProcAddress`), which Cn can supply from the EGL context it
  already built. So `render/aero.rs`'s shader/blur logic ports to C# via
  Silk.NET; only the context bootstrap stays in Cn.

## Status

Planning stage. No C# or Cn code written yet. Real research is done for
the two biggest unknowns (DRM/KMS bindings, Wayland server library), plus
a concrete grep-level map of what `src/handlers/`, `src/shell/`, and
`src/render/` actually lean on Smithay for. The picture is now: **NWayland
replaces the protocol-handler layer (`src/handlers/`), plain C# replaces
the desktop-bookkeeping logic (`src/shell/`, window-stacking bits of
`src/backend/`/`input.rs`), a C# GL binding possibly replaces the GLES
draw calls in `src/render/`, and Cn hand-writes DRM/KMS + session +
libinput device plumbing (`src/backend/udev.rs` and friends).** Both
remaining open questions are now resolved: `xdg-shell`/`xdg-decoration`
ship in core `NWayland` (layer-shell needs the small extra
`NWayland.Protocols.Wlr` package), and GLES draw calls port to
`Silk.NET.OpenGLES` once Cn hands them a live context — only the
EGL/GBM/DRM context bootstrap itself stays in Cn. **The architecture
split is fully decided.**

**Porting has actually started.** `csharp/` now holds a real .NET
solution (`Pancake.sln`): `Pancake.Syrup` is a full, working port of
`src/syrup/` (`SyrupDoc.cs`, `NativeParser.cs` for the native+C-style
frontends, `LuaParser.cs` on NLua for the Lua frontend, `Confi.cs` for
`!lang` detection/dispatch) — 460 Rust lines became ~280 C# lines across
4 files. Verified against the exact example configs from the original
Rust doc comments (native syntax, C-style syntax, Lua syntax, plus the
Lua sandbox actually blocking `os`/`io`) via `Pancake.Syrup.Smoke`, a
throwaway console project that checks 15 real parse assertions — all
pass. One real bug caught in the process: NLua's `LuaTable` enumerates as
`KeyValuePair<object, object>`, not `System.Collections.DictionaryEntry`
— an easy assumption to get wrong coming from Rust's `mlua`, fixed by
switching the `foreach` cast.

Note: the local `dotnet` install's workload-manifest set is broken
(`dotnet workload repair` needs elevated privileges this environment
doesn't have) — worked around with `MSBuildEnableWorkloadResolver=false`,
which is safe for plain class libraries/console apps that don't need
workloads (this project doesn't). Every `dotnet` command in `csharp/`
needs that env var prefixed until the SDK install itself gets fixed.

**`src/config.rs` is also ported now** — `Pancake.Config` (`Config.cs`,
`ReloadSignal.cs`), on top of `Pancake.Syrup` for `.confi` loading and
[Tomlyn](https://github.com/xoofx/Tomlyn) for legacy `.toml` support.
Genuine finding: this file was one of the three originally flagged as
using `unsafe` (for `libc::signal` to install the SIGHUP reload handler),
but .NET's `PosixSignalRegistration` (`System.Runtime.InteropServices`)
covers SIGHUP natively — no FFI, no Cn needed here at all. **The
"3 files use `unsafe`" Cn-surface guess from early on was actually an
overcount by one; only `render/aero.rs` and the DRM/KMS backend files
turn out to be real Cn territory once you look at what the `unsafe` was
actually doing in each file.** Verified with 17 more checks in
`Pancake.Syrup.Smoke` against real temp-directory config files (defaults,
`.confi` load including `startup.apps`, legacy `.toml` fallback) plus a
**real `SIGHUP` sent to the actual running process** and caught by the
handler — 32 checks total across both ported modules, all passing.

## Big finding: Cn might not need to be a separate language at all

Read `src/backend/udev.rs`, `src/backend/gpu.rs`, and all of
`src/render/aero.rs` in full to scope the real Cn surface concretely
instead of by file-level guess. Two things fell out of that:

- **`udev.rs` itself is not low-level.** It's just calloop event-loop
  wiring (session events, DRM VBlank events, libinput events, a repaint
  timer, a Wayland listen socket) — ordinary async orchestration that
  ports to plain C# with no Cn involved. The actual device work is all in
  `gpu.rs`'s `GpuData::init`/`render_all`.
- **`render/aero.rs`'s `unsafe` is 100% plain GLES2 calls** (shader
  compile/link, texture/FBO setup, draw calls) through Smithay's `ffi::`
  module — confirms the earlier Silk.NET.OpenGLES call exactly, now from
  real code instead of a guess.
- **`gpu.rs`'s real low-level work is DRM device/connector/CRTC
  enumeration, GBM buffer allocation, and EGL context creation** — and
  every one of those goes through a **named C shared library with a
  stable, documented API**: `libdrm.so`, `libgbm.so`, `libEGL.so`.
  Confirmed all three exist as normal system shared libraries
  (`ldconfig -p`) — this isn't raw `ioctl()` calls against undocumented
  kernel UAPI structs (which *would* need real hand-packed low-level
  code), it's calling into C libraries that already exist specifically to
  make this safe to call from anywhere, including plain C# `[DllImport]`
  P/Invoke.

**That matters a lot for whether "Cn" needs to exist as a distinct
language at all.** P/Invoke against a named shared library is completely
ordinary, well-trodden C# — `[DllImport("libdrm.so.2")]`, define the
handful of structs libdrm's headers specify, call the functions. There's
no syntactic gap here that a new DSL would close; it's the same
`unsafe`/`DllImport`/`Marshal` machinery C# already has built in. The
earlier framing — "Cn compiles source-to-source into C# `unsafe`
blocks" — was solving a problem (C# can't express raw pointers/FFI
cleanly) that, on inspection of the actual code, doesn't really exist
for *this* codebase. Nothing in `gpu.rs` needs anything C#'s own
`unsafe`+P/Invoke can't already say directly.

**Revised conclusion, pending confirmation:** "Cn" may end up being just
a *naming convention* — the handful of files that do raw P/Invoke against
`libdrm`/`libgbm`/`libEGL` get grouped under a `Cn/` or `*.Cn.cs`
label so it's obvious at a glance which files are the ugly low-level
layer — rather than an actual second compiled language with its own
syntax and source-to-source compiler. That's a substantial simplification
of the original plan (a whole compiler is now off the table) and worth
explicitly confirming rather than assuming.

## Decided: Cn collapses to a naming convention, not a real language

No syntactic gap in the actual code justifies building a second compiler
(P/Invoke against `libdrm`/`libgbm`/`libEGL` is plain, ordinary C#) —
building one anyway would be exactly the kind of premature abstraction/
overbuilt-for-the-task thing to avoid. **`Cn` is now just the label for
"the project that does raw P/Invoke against system C libraries"** — a
`Pancake.Cn` class library, plain C#, `AllowUnsafeBlocks` on, no separate
compiler or syntax. This is a real simplification of the original plan,
not a downgrade: it does exactly what was asked for ("just enough for
what's needed, not a full language") with less to build and maintain.

**Proven on real hardware, not just argued for.** `Pancake.Cn` now has
`Libc.cs`/`Gbm.cs`/`Egl.cs`/`GlesQuery.cs` (`[LibraryImport]` bindings,
the modern source-generated P/Invoke, not the older `[DllImport]`) and
`GpuDevice.cs`, a real port of `GpuData::init`'s GBM+EGL bring-up from
`src/backend/gpu.rs`. This session's sandbox happens to have a real,
world-readable DRM render node (`/dev/dri/renderD128`), so
`Pancake.Syrup.Smoke` opens it for real, creates a real `gbm_device`,
gets a real `EGLDisplay` via `eglGetPlatformDisplay(EGL_PLATFORM_GBM_KHR)`,
creates a real GLES context, makes it current surfaceless, and reads back
real driver strings:

```
GL_VENDOR   = NVIDIA Corporation
GL_RENDERER = NVIDIA GeForce RTX 3050 6GB Laptop GPU/PCIe/SSE2
GL_VERSION  = OpenGL ES 3.2 NVIDIA 610.43.03
```

That's a live NVIDIA context, not a mock — about as strong a proof as
this environment can offer that the GBM/EGL bring-up needs nothing beyond
plain C#. 36 checks total now pass across `Pancake.Syrup`,
`Pancake.Config`, and `Pancake.Cn`. (The test skips gracefully with no
render node present, so it stays portable to machines without a GPU.)

**Connector/CRTC enumeration is now ported and verified too, on real
hardware.** Turns out this sandbox's user *can* actually open
`/dev/dri/card1` (an ACL grants it, despite not being in the `video`
group) — so `Drm.cs`/`DrmResources.cs` (`libdrm.so`'s
`drmModeGetResources`/`drmModeGetConnector`, read-only, no `DRM_MASTER`
needed) got written and tested for real too, not left as a guess. Real
result against real hardware: **4 CRTCs, 1 connector, correctly reported
as disconnected** (no physical display attached in this sandbox — that's
the DRM layer telling the truth, not a bug). Struct layouts
(`DrmModeRes`/`DrmModeConnector`) mirror `xf86drmMode.h` field-for-field
so C#'s `[StructLayout(Sequential)]` reproduces the same alignment/
padding as the C structs.

**A real environment fault showed up mid-session, worth recording
honestly rather than glossing over.** Partway through this work, the
sandbox's GPU itself faulted — `nvidia-smi` started reporting `ERR!`
across fan/temp/power/ECC — and the earlier-working GBM/EGL context
bring-up (`GpuDevice.Open`) started failing with `eglInitialize` /
`EGL_NOT_INITIALIZED`. Confirmed this is a real driver/hardware fault,
**not a Pancake.Cn bug**: reproduced the identical failure with a
minimal Python + `ctypes` script calling the exact same `libgbm`/`libEGL`
functions directly, completely outside .NET — same failure, same
`eglGetError` code. The earlier `GL_VENDOR`/`GL_RENDERER`/`GL_VERSION`
capture (real NVIDIA RTX 3050 output) still stands as proof the binding
is correct; this is a live note that GPU context creation is currently
unavailable in *this* sandbox instance, unrelated to any code here.

Both `render/aero.rs`'s and `src/backend/gpu.rs`'s real low-level
surfaces are now fully ported to `Pancake.Cn` and were verified against
real hardware at least once each this session.

## `src/render/aero.rs` is now fully ported too — `Pancake.Render`

Confirms the earlier call directly: **every `unsafe` call in `aero.rs`
was plain GLES2**, so this whole file ported to ordinary C# with no Cn
involvement at all, using `Silk.NET.OpenGLES` on top of the GL context
`Pancake.Cn.GpuDevice` hands over (`GpuDevice.GetProcAddress` →
`GL.GetApi(...)`, the exact seam sketched out when Cn's scope was first
narrowed).

- `Shaders.cs` — all six GLSL ES shaders (aurora background, wallpaper
  blit, dual-Kawase down/up passes, final blit, glass rect) ported
  **verbatim** — GLSL is portable text, nothing Rust-specific about it.
- `AeroRenderer.cs` — full pipeline port: shader compile/link with real
  error-log surfacing, FBO/texture setup, the ping-pong blur loop,
  `DrawBackground`/`DrawGlassRect`. Matches `aero.rs`'s structure closely
  enough to diff against directly.
- `Wallpaper.cs` — Rust's `image` crate → `SixLabors.ImageSharp`. Pinned
  to **2.1.13**, not the latest 4.x: newer ImageSharp majors moved to a
  paid commercial license above a revenue threshold (Six Labors Split
  License), which doesn't fit an open-source compositor. 2.1.13 is the
  latest patch on the last Apache-2.0-licensed major line with no known
  CVEs (checked via `dotnet add package`'s built-in advisory warnings) —
  worth stating explicitly rather than quietly picking a version.

**Status: builds clean, wired into `Pancake.Syrup.Smoke` to run a real
frame through `Pancake.Cn`'s live context, but not runtime-verified this
session** — the sandbox's GPU fault from earlier in this session (still
showing `ERR!` in `nvidia-smi` as of this check) blocks `GpuDevice.Open`
the same way it blocked the earlier `Pancake.Cn` check, so the pipeline
never got a live context to run against. This is the same pre-existing
environment issue, not a new defect — the code is written and builds,
genuinely untested live is the honest status rather than a false pass.
Re-running `Pancake.Syrup.Smoke` once the sandbox's GPU recovers (or on
real hardware) is the next real verification step, not new porting work.

## `src/shell/layout.rs`'s BSP tiling tree is now ported too — `Pancake.Shell`

Picked this next specifically because it needs **no GPU and no NWayland**
— pure tree/geometry logic, so it's fully testable regardless of the
sandbox's GPU fault above. `Geometry.cs` (`Point`/`Size`/`Rectangle` —
plain int structs; C# doesn't need Smithay's phantom `Logical`/`Physical`
type-parameter trick since this project only ever deals in Logical
space), `Layout.cs` (constants + `TileArea`/`InitialGeometry`), and
`TileTree.cs` (the BSP tree itself: insert/remove/collect-rects/
find-neighbor/swap/adjust-ratio).

**One real design decision, not a mechanical 1:1 port:** `TileTree<TWindow>`
is generic over the window-handle type instead of hardcoding a `Window`
type, because there's no C# window/surface-identity type yet — that
depends on the still-unported `src/handlers/` + NWayland integration.
The tree's own logic never actually needs to know what a window *is*,
only that it's equatable (mirrors the implicit `Window: PartialEq` bound
every `==` in the Rust file relies on), so genericizing it now means this
port doesn't have to be redone once the NWayland-backed window type
exists later — it'll just plug in as `TWindow`. Rust's in-place
`*self = new_tree` reassignment (mutating through `&mut self` to swap in
a different tree variant) became `tree = tree.Insert(...)` /
`(tree, found) = tree.Remove(...)` — C# has no way to change an object's
concrete type in place, so the caller reassigns instead, same as Rust's
own `WorkspaceManager` callers already do when they hold the `&mut
TileTree`.

**Fully verified — 24 real checks, all passing, independent of the GPU
fault**: empty-tree behavior, single/multi-window insert and splitting,
H-split rects sitting side by side with the gap respected, three-way
splits, `FindNeighbor` in all four directions (confirmed against an
actual constructed tree shape, not just "doesn't crash"), `AdjustRatio`
actually growing a window's share, `Swap` actually exchanging two
windows' positions (checked against their pre-swap rects, not just
"still contains both"), `Remove` correctly collapsing the parent split
and leaving the sibling in place, and `InitialGeometry`'s fallback +
cascade-with-count behavior. This is the most thoroughly tested port so
far this session — no environment dependency to blame if something's
wrong, so the bar for "actually verified" was higher and it cleared it.

## Biggest milestone yet: a real Wayland server, in C#, proven over the real wire protocol — `Pancake.Wayland`

This is the load-bearing assumption the entire rewrite depends on:
**can NWayland.Server actually act as a real Wayland compositor server,
speaking the real wire protocol, from C#?** Rather than keep arguing
about it, built the smallest real slice and proved it.

`Pancake.Wayland/PancakeWaylandServer.cs` — a real listening Unix socket,
a real accept loop, `NWayland.Server`'s event-driven dispatch
(`WaylandServer.NextEvent()` returning `WaylandServerSyncEvent`/
`WaylandServerRegistryBindEvent`/`WaylandServerRequestEvent`, per the
library's own documented pattern — found by reading its actual README
and a real sample (`SubcompositorHost`) on GitHub once plain reflection
stopped being enough to infer the intended usage, rather than guessing
at a plausible-looking API and hoping). Advertises a `wl_compositor`
global and implements `CreateSurface`/`CreateRegion` — scoped to match
what `src/handlers/compositor.rs` actually owns (surface/region
lifecycle); the deeper commit/damage-refresh logic in that file depends
on a `Space`/`Window`-equivalent that doesn't exist in C# yet (same
documented gap as `workspace.rs`), so it's out of scope here, not
forgotten.

**Verified with a client that shares zero code with the server**:
`RawWaylandClient.cs` hand-rolls the actual Wayland wire format from
scratch — no NWayland, no libwayland, nothing — message framing
(object id + `size<<16|opcode` header), `string`/`uint`/fixed `new_id`
argument encoding, and `wl_registry.bind`'s special *dynamic* new_id
encoding (`string interface, uint version, uint id`, since bind's target
interface isn't fixed by the protocol XML). This matters: if both sides
had been NWayland, a pass would only prove "NWayland agrees with
itself." A client that speaks the format independently and gets a
correct response proves the wire protocol itself works.

**Real sequence, real result:** connect → `wl_display.get_registry` +
`wl_display.sync` → read real `wl_registry.global` events → confirm
`wl_compositor` was actually advertised → `wl_registry.bind` it →
`wl_compositor.create_surface` → confirm no `wl_display.error` came back
→ confirm the server's own surface counter actually incremented. All 4
checks pass. This is the biggest single validation of the session: the
foundation the entire `src/handlers/` port will stand on is now proven,
not assumed.

**Extended immediately to the real xdg-shell toplevel handshake** —
`XdgShell.cs` adds `xdg_wm_base`/`xdg_surface`/`xdg_toplevel`/
`xdg_positioner`/`xdg_popup` listeners. Scope again matches
`src/handlers/xdg_shell.rs` precisely: the wire-protocol object lifecycle
and the `configure`/`ack_configure` handshake are real; everything
`new_toplevel`/`toplevel_destroyed` do with the *result* (space mapping,
workspace registration, focus, retiling, move/resize grabs) is out of
scope, same documented `Space`/`Window` gap as before — popups are wired
enough to satisfy the protocol (a client is entitled to call
`create_positioner` even unused) but don't implement real placement.

Extended `RawWaylandClient` with the same discipline — no shortcuts, no
borrowing NWayland's own client half: `bind(xdg_wm_base)` →
`get_xdg_surface` → `get_toplevel` → `wl_surface.commit` → **read real
`xdg_surface.configure` and `xdg_toplevel.configure` events back** →
`ack_configure`. This is the actual handshake that turns a bare
`wl_surface` into a real window in every Wayland compositor that exists —
the single most load-bearing protocol exchange in the entire rewrite.
**All 5 new checks pass**: both configure events received, no protocol
error across the full exchange, and the server's own toplevel counter
confirms real creation. 9 total `wayland:` checks now pass.

**`src/handlers/xdg_decoration.rs` is fully ported too — no gaps this
time.** Unlike `compositor.rs`/`xdg_shell.rs`, this handler never touches
`Space`/`Window` at all — it's pure protocol-level mode negotiation
("should the client draw its own title bar or should the compositor"),
so `XdgDecoration.cs` is a **complete** port, not a wire-protocol-only
slice with app logic deferred. Ported the actual policy faithfully:
`GetToplevelDecoration` always grants `ClientSide` (Pancake doesn't draw
server-side title bars yet), `SetMode(ServerSide)` gets downgraded to
`ClientSide`, `UnsetMode` falls back to the same default — all three
real `Rust` code paths, not just the object lifecycle.

Verified the same way, real policy and all: bind the manager, request a
decoration on the toplevel from the earlier xdg-shell handshake, and
check the real `configure(mode)` events the server sends back —
confirmed a fresh decoration defaults to client-side (mode `1`), a
`ServerSide` (`2`) request actually gets downgraded back to `1`, and
`unset_mode` also lands on `1`. All 5 new checks pass — 14 total
`wayland:` checks now pass.

**`src/handlers/layer_shell.rs`'s wire-protocol layer is ported too** —
`LayerShell.cs`, the `zwlr_layer_shell_v1`/`zwlr_layer_surface_v1`
protocol waybar/dunst/wofi actually speak for panels and notifications.
Needed a new package, confirming an earlier finding on real code instead
of just a `.csproj` read: layer-shell isn't in core `NWayland` (it comes
from `wlr-protocols`, a separate upstream repo from `wayland-protocols`),
so this required adding `NWayland.Protocols.Wlr` — exactly as predicted
back when the NWayland library comparison was first done. Same split as
`xdg_shell.rs`/`compositor.rs`: the object lifecycle and
`configure`/`ack_configure` handshake are real; computing actual
geometry from output size + anchor/margin/exclusive-zone and reserving
space for other layers needs the still-unported `Space`/`Output`
equivalent, so `Configure` currently sends a placeholder `(0,0)` size —
same honest pattern as `xdg_toplevel`'s initial configure before real
layout exists.

Verified with the same independent client: create a fresh `wl_surface`,
bind `zwlr_layer_shell_v1`, request a layer surface on the `Overlay`
layer (matching Pancake's own panel), commit, and read the real
`configure(serial, width, height)` event back, then `ack_configure` it.
All 3 new checks pass — **17 total `wayland:` checks now pass** across
`wl_compositor`, `xdg_shell`, `xdg_decoration`, and `layer_shell`.

**Hit a real fork point after this.** The two remaining `src/handlers/`
files aren't like the four above — `input.rs`'s `SeatHandler` impl is
nearly empty (two no-op callbacks); its real substance is all keybinding/
focus/grab policy touching `self.space` 26 times with no wire-protocol
slice left once that's stripped out. `xwayland.rs` is worse: every real
handler calls straight into `self.space`, and XWayland isn't even
Wayland protocol — it's a separate X11 window-manager role NWayland
doesn't cover at all. Both dead-end at the same missing piece: **there
was no C# `Space`/`Window`/`Output` container**, so raised this
explicitly instead of quietly picking a direction — asked whether to
design that container next, stop, or commit first. Chose to build it.

## The missing piece: `Pancake.Shell`'s `PancakeWindow`/`PancakeSpace`/`WorkspaceManager`

`Window.cs` — `PancakeWindow`, a plain reference-equality class (matches
Smithay's `Window` being an `Rc`-backed handle compared by pointer
identity — every `w == window` in the Rust codebase relies on that, so a
C# class's default reference equality is the direct equivalent, no
wrapper needed). `Output.cs` — `PancakeOutput`, just name + geometry
(physical properties/modes belong with the DRM/KMS output enumeration in
`Pancake.Cn`, not here). `Space.cs` — `PancakeSpace`, covering exactly
what the handler files actually call: `MapElement`/`UnmapElement`/
`RaiseElement`/`ElementGeometry`/`Elements()`/`Outputs()`/
`OutputGeometry`/`Refresh` (a documented no-op until the damage/render
pipeline that would drive it exists).

**`src/shell/workspace.rs` is now fully ported too** —
`WorkspaceManager.cs`, the reason this file couldn't be ported earlier
this session. Uses `TileTree<PancakeWindow>`, the exact generic
instantiation `TileTree.cs` was built for back when the BSP tiling tree
was ported standalone. All of it: 9-workspace switching (unmap old,
remap new), move-window-to-workspace, tiling toggle/apply, ratio
adjustment, neighbor swap — the complete original file, not a slice.

**Verified with 21 real checks, no GPU needed** (same rigor as the
tiling-tree port): `PancakeSpace` — mapping, raising, unmapping, geometry
tracking, output registration, all checked against real before/after
state. `WorkspaceManager` — switching workspaces actually unmaps/remaps
the right windows in the real space, `move_window_to` actually relocates
between workspaces, tiling actually gives windows real non-zero
geometry, `remove_window` actually finds the right workspace.

**Then closed the loop for real**: wired `Pancake.Wayland`'s
`XdgWmBaseListener`/`XdgToplevelListener` to the real
`PancakeSpace`/`WorkspaceManager` instead of just the wire handshake —
completing `xdg_shell.rs`'s `new_toplevel`/`toplevel_destroyed` app logic
that was deferred earlier this session, not just the protocol object
lifecycle. A real toplevel creation now: computes cascaded initial
geometry via `Layout.InitialGeometry` (using real registered outputs if
any, the documented fallback otherwise), maps and raises the window in a
real `PancakeSpace`, registers it in the active workspace splitting the
previously-focused window if tiling is on. A real `xdg_toplevel.destroy`
now correctly unmaps it and removes it from the workspace. Verified with
the same independent hand-rolled client, end to end: create the real
toplevel handshake → confirm the server's own `Space`/`WorkspaceManager`
actually gained one window with real fallback geometry (`960×600`, no
outputs registered on this test server) → send a real `destroy` request
→ confirm both `Space` and `WorkspaceManager` are empty again. All
checks pass.

**23 total `wayland:` checks now pass** (up from 17), plus the 21 new
`space:`/`workspace:` checks — **44 real checks added this round.**

## The `wl_seat`/`wl_pointer`/`wl_keyboard` gap closed too — `Seat.cs`

The exact remainder flagged above. Worth being precise about what this
is: **not a port of `input.rs`** — that file's own `SeatHandler` impl is
two no-op callbacks (`cursor_image`, `focus_changed`); the actual
`wl_seat` global and `wl_pointer`/`wl_keyboard`/`wl_touch` object
creation is something Smithay's `SeatState`/`Seat::new` do generically,
outside `input.rs` entirely. So `Seat.cs` is genuinely new wire-protocol
work (`wl_seat` global, `GetPointer`/`GetKeyboard`/`GetTouch`/`Release`,
real `Capabilities`/`Name` events on bind), not a translation of existing
Rust. What `input.rs` *does* contain — keybinding interception,
click-to-focus, move/resize grab tracking — all needs real hardware
input events, which need Pancake.Cn's still-unbuilt libinput layer, so
none of that is here; only the object lifecycle these features would
eventually run on top of.

Verified the same way: bind `wl_seat`, read the real `capabilities()`/
`name()` events back (confirmed `Pointer`+`Keyboard` capabilities and a
non-empty seat name), then request pointer/keyboard/touch objects and
confirm no protocol error. **5 new checks pass — 28 total `wayland:`
checks now pass.**

**Where `input.rs` and `xwayland.rs` actually stand now:** `input.rs`'s
real remaining gap is exactly one thing — real hardware input events
(needs Cn's libinput layer, unbuilt). `xwayland.rs` remains blocked on a
completely different library (X11 protocol, not Wayland) and is out of
scope for NWayland entirely.

## `Pancake.Cn`'s libinput layer — real bring-up, real bug found and handled properly

Started the piece `input.rs` was actually waiting on: `Libinput.cs`/
`InputDevice.cs`, using libinput's "path" backend
(`libinput_path_create_context`/`libinput_path_add_device`, real
`open_restricted`/`close_restricted` callbacks via `[UnmanagedCallersOnly]`
function pointers) against this sandbox's real, accessible
`/dev/input/eventN` nodes — same "real slice of the real library"
approach as `Gbm.cs`/`Egl.cs`/`Drm.cs`.

**Hit a real, reproducible segfault, and ran it down properly instead of
hiding it.** `libinput_dispatch()` crashes in this sandbox — confirmed
independently in **raw Python + `ctypes`**, completely outside .NET, on
**every** `/dev/input/eventN` node tried. Isolated the exact failure
point by testing each libinput call in sequence: `create_context` →
`add_device` → `get_fd` all succeed cleanly; `dispatch()` is where it
crashes, every time. `libinput.so`'s package dependencies (`ldd`) show
it links `libwacom.so` for tablet device-capability probing, which
happens on first dispatch, not on add — the likely culprit, though the
underlying cause is in the sandbox's environment, not something to chase
further here.

**A real segfault can't be caught with a managed `try`/`catch`**, so
`libinput_dispatch()` needed to be isolated in a subprocess rather than
called in-process — otherwise this one crash would silently kill every
check that runs after it in `Pancake.Syrup.Smoke` (which is exactly what
happened on the first attempt: the process died with exit code 139
partway through, and everything after the libinput section — all the
`shell:`/`workspace:`/`wayland:` checks — silently never ran). Added a
`--libinput-dispatch-check <path>` subprocess mode to the smoke-test
executable itself; the parent process spawns it, waits, and reports the
child's exit code as a finding rather than propagating the crash. Fixed
and verified: the full suite (all ~90 checks) now runs to completion
every time, and the libinput section correctly reports what's real
(`context`/`add_device` succeed) and what's blocked (`dispatch` crashes,
with the diagnosis) instead of either crashing everything or silently
passing a check that didn't actually run.

## `src/render/borders.rs` ported too — another gap `PancakeSpace` closed

Same story as `workspace.rs`: this file was never actually GPU-side
logic (it just computes four colored rectangles per window from `Space`
geometry, handed to a renderer), so it was blocked purely on
`PancakeSpace` not existing — now that it does, `Borders.cs` is a
**complete** port, not a wire-protocol/app-logic split. Verified with 5
real checks: two windows produce exactly 8 border strips (4 each), the
focused window's strips get the real active-amber color while the other
gets the real inactive-slate color (checked by actual RGB values, not
just "some color"), the top strip sits at the real computed position
just above the window, and `output_scale` actually changes border
thickness (checked `1.0` vs `2.0` scale producing different real pixel
values). All pass.

## `src/render/cursor.rs` ported too — real xcursor parsing, verified against this system's actual installed cursor themes

Another fully self-contained file (no `Space` dependency, never was
blocked on it) — `Cursor.cs` + `XcursorFile.cs`. Rust's `xcursor` crate
becomes a hand-rolled binary parser here, same "read the real bytes, no
external dependency" approach as `RawWaylandClient`'s wire-protocol work:
the Xcursor file format (X.Org's XCURSOR spec) is small and has been
stable for decades, so there's no real gap a library would close that
directly reading the header/table-of-contents/image-chunk bytes doesn't.
**Checked the exact byte layout against a real installed cursor file
first** (`/usr/share/icons/Adwaita/cursors/default`, via a quick Python
`struct`-based reference parse) before writing the C# version, same
verify-the-wire-format-against-real-data discipline as the Wayland work.

Theme resolution is a documented, bounded simplification: checks the
conventional cursor-theme directories directly (`~/.icons`, `~/.local/
share/icons`, `/usr/share/icons`, `/usr/local/share/icons`) rather than
walking a theme's `index.theme` `Inherits=` chain the way real
freedesktop icon-theme resolution (and the real Rust `xcursor` crate)
does — same "real for the common case, not a partial imitation of the
full spec" pattern as `place_meeting`'s AABB collision or the
always-copy array semantics from earlier in this project's Butterscotch
work.

**Verified against this environment's real, installed system cursor
themes, not synthetic data** — `Adwaita`/`Yaru`/etc. genuinely exist
under `/usr/share/icons` here. Forced `XCURSOR_THEME=Adwaita` and loaded
the real default cursor: got back **exactly** `24×24`, hotspot `(3,1)` —
matching the reference Python parse byte-for-byte. Also verified the
built-in-arrow fallback path for real by forcing an impossible theme
name, and confirmed the fallback bitmap actually has real black/white
opaque pixels, not just non-zero dimensions. **8 new checks, all pass.**

## `src/render/decorations.rs` ported too — title bars, buttons, and the real click hit-test

`Decorations.cs` — the last real gap `PancakeSpace` closed. Same shape
as `borders.rs` (title bar + close/minimize/maximize dots as colored
rectangles), but also `hit_test`, the function that turns a pointer
click into "which window, and which part of its decoration" — the thing
`input.rs` would eventually call to handle clicking a window's close
button. Needed `Rectangle.Contains(Point)` on `Pancake.Shell`'s geometry
type, which didn't exist yet (nothing had needed hit-testing before this
file), so added it there rather than duplicating point-in-rect math
locally.

**8 new checks, all pass**, including real hit-test behavior: a click in
the middle of a title bar returns `TitleBar`, a click on the actual
close-button rectangle returns `Close` (not just "some hit" — checked
against the real computed button position), a click outside any window
returns nothing, and — a real edge case, not just the happy path — a
click *inside* a window's content area (below its title bar) correctly
returns nothing too, matching `decorations.rs`'s actual scope (it only
covers the bar, not the window content itself).

This closes out `src/render/` almost entirely: `aero.rs` (built,
GPU-fault-blocked in this sandbox), `borders.rs`/`cursor.rs`/
`decorations.rs` (all complete). Only `mod.rs` remains — a thin
Smithay-`render_elements!`-macro glue file with no C# equivalent needed
yet (that's real GPU render-dispatch plumbing tied to a render-element
abstraction Pancake.Render doesn't have built beyond `AeroRenderer`'s
specific pipeline).

## `state.rs`'s orchestration logic ported too — `PancakeCompositorState`

Read `state.rs` (the central struct tying the whole compositor together)
looking for what's portable now that `PancakeSpace`/`WorkspaceManager`
exist, and found most of its *action* methods — `retile`,
`toggle_tiling`, `focus_tile`, `swap_tile`, `resize_tile`, `cycle_focus`,
`snap_focused` — turned out to be almost entirely `Space`/
`WorkspaceManager` orchestration with no GPU or protocol dependency once
those two existed. `CompositorState.cs`'s `PancakeCompositorState` ports
all of them.

**Honest about the one real thing left out**: every method updates
`FocusedWindow`, but the real `keyboard.set_focus(...)` wire call (which
would send an actual `wl_keyboard.enter`/`leave` event to a client) isn't
wired up — that needs a live `wl_keyboard` resource tied to focus
changes, and while `Seat.cs` creates real `wl_keyboard` objects, nothing
drives them yet since no input events exist to trigger a focus change in
the first place (blocked on the same libinput/sandbox issue documented
above). That's a concrete, well-scoped gap now, not a vague "input.rs
isn't done."

**12 real checks, all pass**, including catching a real test-setup bug
along the way (not a port bug): the first version of the `toggle_tiling`
test mapped windows directly into `Space` without registering them via
`Workspaces.AddWindow` first, which isn't valid usage — `ToggleTiling`
rebuilds its tiling tree from `WorkspaceManager`'s own window list, not
from `Space` directly, exactly matching how `xdg_shell.rs`'s
`new_toplevel` always does both together. Fixed the test to match real
usage rather than loosening the check. Covered: `cycle_focus` advancing
and wrapping around a real window list, all four `snap_focused`
directions producing real geometry (left/right halving the output,
up maximizing — checked against actual computed sizes), tiling
producing real non-zero geometry, and `focus_tile` actually moving focus
to a real BSP-tree neighbor.

## `main.rs` ported too — `Pancake.App`, a real (if honestly incomplete) entry point

The actual CLI entry point: arg parsing (`--winit`/`--tty`), SIGHUP
install, backend dispatch. Small, but a real capstone — it's the first
thing that ties multiple projects together into one running program
instead of a library exercised only by the smoke-test harness.

**Deliberately honest about what is and isn't wired up, rather than
faking a "working compositor."** `--winit` prints a clear message that
`src/backend/winit.rs` (Smithay's nested-window dev-mode backend) has no
C# port yet — a real, tracked gap, not a silently wrong success exit.
The default (udev) path is real: loads real config via `Pancake.Config`,
starts a real `Pancake.Wayland.PancakeWaylandServer` on a real socket
path under `$XDG_RUNTIME_DIR`, and **prints an explicit note** that
`Pancake.Cn.GpuDevice` and `PancakeWaylandServer` both exist and are each
independently verified, but nothing this session connected them into one
frame loop — a client can connect and create real windows (proven by the
`wayland:` checks), but nothing renders them to a screen yet.

**Verified for real, not just build-checked**: ran it with a timeout and
confirmed, while it was running, a real Unix socket (`srwxr-xr-x`)
actually existed at the real runtime path
(`/run/user/1000/pancake-wayland-0`) — not mocked, an actual `bind()`ed
socket a real Wayland client could connect to.

## `src/backend/winit.rs` ported too — a real nested-window dev-mode backend, with a real bug found and fixed

`--winit` originally just printed "not ported." Went back and built it
for real: `NestedBackend.cs` uses `Silk.NET.Windowing` (GLFW/SDL-backed)
instead of hand-writing window-system integration — unlike DRM/KMS,
X11/Wayland protocol negotiation is genuinely complex enough that a
mature library is the right call, not a Cn-style P/Invoke hand-roll.
Creates a real window, a real GLES context, and runs the real
`AeroRenderer` blur pipeline every frame. Border/decoration/cursor
compositing isn't included — needs a small solid-quad shader this
session didn't build, a real bounded gap, not silently skipped.

**Found a second real environment issue, distinct from the earlier one,
and fixed the code around it.** This sandbox's real Wayland socket
(confirmed reachable — connected to it directly with a raw Python
socket first) accepts the window connection fine, but GL context
creation **hangs instead of failing cleanly** — a different failure mode
than the GBM/EGL path in `Pancake.Cn`, which hits the same underlying
GPU fault but returns a clean error immediately. Diagnosed this by
adding temporary trace prints at each stage (window create → initialize
→ Load → GL context) to find exactly where execution stalled, confirming
it's specifically GL-context creation inside `Load`, not window/display
setup.

A hang with zero feedback is a real defect regardless of the underlying
cause, so fixed it properly rather than just noting it: `window.Run()`
now runs on a background thread while the main thread watches for a
`Load`-fired signal with a 5-second timeout, reporting a clear diagnostic
and exiting cleanly if it's not met — verified this actually works (real
5-second wait, clean exit code 1, process doesn't hang) rather than
assuming the fix is correct. `--winit` now behaves the same way the DRM
path already does: real code, real attempt, clean and honest reporting
of the sandbox's GPU fault instead of a silent hang.

## `wl_data_device_manager` (clipboard/drag-and-drop) wire objects — the last real protocol surface

Checked `state.rs` for what else it delegates that hadn't been touched
yet: `DataDeviceHandler`/`SelectionHandler`/`ClientDndGrabHandler`/
`ServerDndGrabHandler`. All four have **literally empty method bodies**
in Pancake's own Rust source — every real mechanic (fd-forwarding
clipboard data between clients, drag-grab overlays, cross-client
selection broadcast) lives inside Smithay's own `DataDeviceState`
internals, not in anything state.rs itself wrote. So `DataDevice.cs`
is scoped to exactly what's actually Pancake's: the
`wl_data_device_manager`/`wl_data_device`/`wl_data_source`/
`wl_data_offer` object lifecycle, same shape as `Seat.cs` — not a
reimplementation of Smithay's internal clipboard-proxying machinery,
which would be inventing new infrastructure rather than porting
anything that exists in this codebase.

Verified the same way: bind the manager, create a data source and a
data device for the seat, confirm no protocol error. **2 new checks
pass.** This closes out every `wl_*`/`xdg_*`/`zxdg_*`/`zwlr_*` global
`state.rs` advertises — the full Wayland protocol surface this
compositor's Rust source actually touches is now represented in
`Pancake.Wayland`.
