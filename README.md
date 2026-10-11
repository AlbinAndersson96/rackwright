# Rackwright

A browser app for planning and documenting rack-mounted test stations in 3D. You build a
station from racks, place measurement equipment in them, add named ports to the equipment, and
see it all rendered live in a WebGPU 3D view.

It is written in Rust, compiled to WebAssembly, and runs entirely in the browser. There is no
server: your work is autosaved in the browser and can be exported to or imported from JSON
files.

## Features

- Several racks side by side (height configurable per rack, up to 100U). Rack order is changed
  by dragging rows in the tree.
- Equipment with a name, height (U) and colour. New equipment is dragged from the sidebar into a
  rack; dragging onto other equipment pushes it out of the way. Equipment can be moved within a
  rack or to another rack.
- Ports on a device's front face, on a grid of 5 cells per U. They are placed by dragging, with
  the camera zooming to the face.
- Names printed in 3D: rack names on the header, device names on the front, and port names when
  a port or its device is selected or hovered.
- Selection kept in sync between the tree and the 3D view. Double-click frames an object.
  Right-click offers Rename and Delete. Dropping on the trash zone deletes.
- Undo/redo of every change (Ctrl+Z, Ctrl+Y / Ctrl+Shift+Z).
- Autosave to the browser's local storage, JSON export and import, and recovery if stored data
  can't be read.

The full behaviour is specified in
[`docs/superpowers/specs/2026-10-05-rackwright-mvp-design.md`](docs/superpowers/specs/2026-10-05-rackwright-mvp-design.md).

## Requirements

- **To use it:** a browser with WebGPU, such as a current Chrome or Edge. Firefox has WebGPU on
  Windows and recent macOS versions, and Safari from Safari 26. The page must be served over
  HTTPS or from `localhost`.
- **To build it:**
  - Rust (stable, edition 2024).
  - The WebAssembly target: `rustup target add wasm32-unknown-unknown`.
  - Trunk: `cargo install trunk --version 0.21.14 --locked`.
- **To run the GPU tests:** a Vulkan driver. On Linux, including WSL, Mesa's software driver is
  enough (Ubuntu package `mesa-vulkan-drivers`). Without an adapter these tests skip
  themselves.

## Running

```bash
cd crates/app
trunk serve --release        # http://localhost:8080
```

To build a static site for hosting (any static HTTPS host works):

```bash
cd crates/app
trunk build --release        # output in crates/app/dist/
```

## Developing

```bash
cargo test --workspace                                   # all tests, run natively
cargo test -p rackwright-core --test placement                  # one test file
cargo test -p rackwright-app --test interaction drag            # tests whose name contains "drag"
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p rackwright-app -p rackwright-render --target wasm32-unknown-unknown -- -D warnings
```

The workspace has three crates:

| Crate | Path | What it contains |
|---|---|---|
| `rackwright-core` | `crates/core` | Document model and every editing rule (placement and push, naming, validation), undo/redo, and the JSON file format. Pure Rust. |
| `rackwright-render` | `crates/render` | World layout, picking, camera, scene building and the wgpu renderer. |
| `rackwright-app` | `crates/app` | Session, interaction controller, storage and forms (pure Rust), plus the Leptos UI (browser only). |

The design document and the three implementation plans that built the MVP are in
`docs/superpowers/`.

Changes go on a feature branch and reach `develop` through a pull request.

## Known issues and future work

- Small deferred review findings: [`docs/known-issues.md`](docs/known-issues.md).
- What was left out of the MVP, planned directions (equipment catalogue, connector types,
  cables, shared backend) and where the code already prepares for them:
  [`docs/future-work.md`](docs/future-work.md).
