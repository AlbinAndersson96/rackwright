# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo test --workspace                         # all tests (native); GPU tests need a Vulkan adapter
cargo test -p rackwright-core --test edit_device      # one integration-test file
cargo test -p rackwright-app --test interaction drag  # tests matching a name filter
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p rackwright-app -p rackwright-render --target wasm32-unknown-unknown -- -D warnings   # browser-only code
cargo check -p rackwright-app --target wasm32-unknown-unknown
cd crates/app && trunk serve --release          # dev server, http://localhost:8080
cd crates/app && trunk build --release          # static site in crates/app/dist/
```

- **Run both clippy commands before committing.** `ui/`, `text::CanvasTextRasterizer`,
  `gpu::Renderer::for_canvas` and `storage::LocalStorageStore` are `#[cfg(target_arch =
  "wasm32")]`, so native builds never compile them.
- **There is no browser in this environment.** The 3D view can only be verified by the user in
  their Windows browser (WSL serves `localhost:8080` to Windows). Headless Chrome on Linux
  cannot present WebGPU canvases.
- **GPU tests serialise themselves.** `crates/render/tests/gpu.rs` renders offscreen on Mesa's
  llvmpipe and holds a process-wide lock, because creating Vulkan devices concurrently segfaults
  in that driver. Keep new GPU tests behind `gpu_lock()`.

## Architecture

Three crates, each depending on the previous. The rule throughout: **logic lives in pure Rust
with native tests; browser code is a thin layer.**

- **`rackwright-core` (`crates/core`): the document and every editing rule.**
  - Every edit is a `plan_*` function in `edit/` that takes `&Document` and returns
    `Result<Plan, Rejection>`.
  - A `Plan` holds a complete candidate document plus the `subject` it created or moved.
  - Rejections never change anything. Plans are used both for live drag previews and for
    commits.
  - `Editor` keeps whole-document undo/redo snapshots.
  - `placement.rs` (1D device push) and `port_grid.rs` (2D port push) are the pure algorithms
    behind the plans.
  - `validate` checks every invariant. `file_format` is the versioned JSON format, with on-disk
    types kept separate from the model.
  - Configurable limits (name length, port grid, max rack height) live only in `Limits`.
- **`rackwright-render` (`crates/render`): world geometry and drawing.**
  - World units are millimetres, Y up, racks along +X with their fronts at z = 0. U1 sits on the
    plinth. Port row 0 is the bottom row of a device face. Cables are drawn as tube segments
    (`TubeInstance`) along `layout::cable_path` between port anchors.
  - `layout` (geometry), `pick` (rays, picking, drop targets), `camera` (orbit camera, tweens),
    `scene` (builds draw lists from a document plus selection, hover and ghost; eases moved
    objects) and `view` (camera, tweens, port-placement mode) are pure and tested natively.
  - `gpu::Renderer` (wgpu 30, WebGPU in the browser, Vulkan for tests) draws a `Scene`.
    `TextRasterizer` abstracts label text; the browser implementation uses an `OffscreenCanvas`.
- **`rackwright-app` (`crates/app`): the application.**
  - `session::Session` holds the editor, selection, view, drag preview, banner and startup
    problem. Every user action is a method on it.
  - `interaction.rs` adds the pointer/keyboard state machine as further `impl Session` methods.
    Positions are canvas-relative CSS pixels.
  - `revision()` changes with the document and drives autosave. `ui_revision()` changes with
    anything the DOM shows.
  - `storage` holds the `DocumentStore` trait (JSON text, async so a backend can replace local
    storage), startup recovery and autosave.
  - `ui/` (Leptos 0.8, client-side rendering, browser only) keeps non-reactive state in a
    thread-local `Core` (`ui/core.rs`).
  - **UI pattern:** components read through `read(|s| …)` after tracking `sig.rev`. They mutate
    only through `update(|s| …)`, which syncs signals, autosaves on a revision change and
    schedules a `requestAnimationFrame`. Frames are drawn only while something animates.

The design spec (`docs/superpowers/specs/`) is the authority on behaviour. The three plans in
`docs/superpowers/plans/` record how the MVP was built and the decisions taken beyond the spec.
Deferred minor review findings are in `docs/known-issues.md`. Planned directions and their extension points (`DeviceKind`, `PortKind`, `DocumentStore`, `Limits`, file-format migration) are in `docs/future-work.md`. Read it before extending the model or storage.

## Workflow

- `develop` is the base and default branch. Work goes on a feature branch and is merged through
  a pull request (`gh` is installed and authenticated).
- New behaviour is built test-first. For logic, the test goes in the owning pure module's
  native tests. Browser-only glue gets a manual check instead, because there is no headless
  WebGPU here.
