# Working on VisualReplayDebuggerCraft

A Rust rewrite of VisualReplayDebugger, modelled on SolidCraft: a Cargo workspace, an egui UI that runs on
desktop and in the browser, wgpu rendering. Reference checkouts: `C:\Projects\VisualReplayDebugger` (old C#
tool) and `C:\Projects\solidcraft`.

## Layout
* `crates/format`: `.vrd` reader/writer. See `docs/format.md`. Must keep loading existing captures.
* `crates/model`: `Replay`, the indexed view of a capture (entities, timelines, logs, draws, frame/time).
* `crates/engine`: `Playback` (time, frame, speed, stepping). No UI code.
* `crates/scene`: CPU side of the 3D viewport (camera, tessellation, `build_frame`, picking). No GPU code, fully unit tested.
* `crates/render`: wgpu renderer that draws a `SceneFrame` into an offscreen texture. Headless GPU tests read pixels back.
* `crates/ui-egui`: the UI (`ReplayApp`), backend-agnostic so it runs natively, on the web and in tests. `viewport.rs` is the 3D panel.
* `apps/vrd` desktop (eframe + wgpu), `apps/vrd-web` browser (trunk), `apps/vrd-cli` inspect/convert.
* `testdata/sample.vrd`: a real capture used by tests.

## Commands
* `cargo test --workspace`
* `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all`
* `cargo run -p vrd -- testdata/sample.vrd`
* `cargo run -p vrd-cli -- render testdata/sample.vrd --out out.png --frame 600` draws a frame headlessly. Use it to look at rendering changes. `info` and `entities` print what a capture contains.
* `cd apps/vrd-web && trunk serve`, then open `/?url=sample.vrd`
* The web app deploys to https://gitdonohue.github.io/VisualReplayDebuggerCraft/ from `.github/workflows/pages.yml` on every push to `develop` or `main` (Pages source must be "GitHub Actions"). To test the subpath build locally: `trunk serve --release --public-url /VisualReplayDebuggerCraft/`.

## Windows
Rust must link with a complete MSVC toolchain. If `cargo` picks a Visual Studio install without `msvcrt.lib`,
run it from a "x64 Native Tools" prompt for VS 2022.

## 3D conventions
Captures carry no units or axis conventions, so the viewport detects them (`docs/viewport.md`). Never hard-code a scale such as "100 = 1 m" or "Z is up": use `SceneAssets::unit()` and the camera's `up_axis`.

## Gotchas
* eframe's `wgpu` feature selects no graphics backend. The apps add `wgpu` with explicit backends; without
  that, startup panics.
* Keep logic out of `ui-egui` when it can be tested without egui (see `log_filter.rs`, `vrd-engine`).
* The renderer outputs an `Rgba8Unorm` texture holding sRGB-encoded colour (the shader encodes). egui expects gamma-encoded images; an `*Srgb` format makes the viewport look too dark.
* wgpu paints with reversed-Z (depth cleared to 0, compare `Greater`, infinite far plane).
* Tool calls that read a file you are still writing in the same step race. Render first, then read.