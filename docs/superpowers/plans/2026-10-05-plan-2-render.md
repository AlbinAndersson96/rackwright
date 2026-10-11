# TestStationView Plan 2 — `render` crate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `tsv-render` crate: world layout, picking and drag targeting, orbit camera with tweens, draw-list building with object motion, view state with port-placement mode, and a WebGPU renderer (instanced boxes, text labels, translucent ghost, selection glow). It runs natively for tests and compiles for the browser.

**Architecture:** Everything except `gpu` and the browser text rasteriser is pure Rust over `tsv-core` types and fully unit-tested natively. `gpu::Renderer` draws a `scene::Scene` (built from a document plus selection, hover and ghost) through wgpu. Its pixels are verified on a real adapter: Mesa's `llvmpipe` software Vulkan device works headless, including in WSL. Plan 3 (`app`) will own the canvas, the frame loop and input, and will call into this crate.

**Tech Stack:** Rust 1.99 (edition 2024), `wgpu` 30 (features `std`, `wgsl`, `webgpu`, `vulkan`; no WebGL), `glam` 0.34, `bytemuck` 1, `web-sys`/`wasm-bindgen` (wasm32 only), `pollster` (tests).

**Spec:** `docs/superpowers/specs/2026-10-05-rackwright-mvp-design.md` (§4.2 is the renderer). Plan 1 (`docs/superpowers/plans/2026-10-05-plan-1-core.md`) is merged; its API is what this plan consumes.

This is plan 2 of 3. All code in this plan was compiled and its tests run (native and `wasm32-unknown-unknown` checks) in a scratch prototype before the plan was written. If a step's output differs from `Expected:`, suspect the environment first, for example a missing GPU driver or a different crate version resolved by Cargo.

## Global Constraints

- Work on a feature branch from `develop` (for example `plan-2-render`). Integrate by pull request into `develop`.
- Package `tsv-render` in `crates/render`, imported as `tsv_render`. It depends on `tsv-core` by path.
- `wgpu` is built with `default-features = false, features = ["std", "wgsl", "webgpu", "vulkan"]`. In the browser only WebGPU is used (`Backends::BROWSER_WEBGPU`); natively, `Backends::PRIMARY` (Vulkan) is for tests. No WebGL fallback (spec §1).
- World units are millimetres. Right-handed, Y up. Racks stand in a row along +X; their front plane is z = 0 and they extend towards -Z. U1 sits directly on the plinth. 1U = 44.45 mm; device fronts are 482.6 mm wide.
- Camera `yaw = 0, pitch = 0` looks straight at the rack fronts. The default front view ("Reset view") uses pitch 10°. All programmatic camera moves tween over 0.4 s with smoothstep easing.
- Moved devices and ports ease to their new positions with an exponential approach (`MOTION_SECONDS` = 0.12 s to about 95 %).
- Colours are written to a non-sRGB target, so `Rgb` values appear as specified. Labels are black or white, by background luminance.
- Selection: an amber glow (`#ffa600`, 3 px, fading) outside the selected object's silhouette. Selecting a rack outlines all its frame parts.
- Ghost: translucent (alpha 0.45), in the device's colour when valid and red `#e62828` when the drop would be rejected.
- Every commit message ends with the trailer line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Pointer exactly on an object's edge.** An axis-aligned ray whose origin lies exactly on a box face must still hit, not produce NaN and miss. Pinned in Task 2 (`pointer_exactly_on_a_box_edge_still_picks_cleanly`). The prototype had this bug, and the slab test now treats parallel axes explicitly.
2. **Empty document** (every rack deleted). Picking and drop targeting must return nothing, and framing must still produce a sensible camera. Pinned in Task 2 (`empty_document_picks_nothing`), with `scene_bounds` falling back to a default rack in Task 1.
3. **Largest allowed document** (5 racks × 100U). Reset view must fit everything without hitting the zoom clamp. Pinned in Task 3 (`largest_document_fits_the_front_view`).
4. **Zero-size canvas** (hidden tab, collapsed layout). Resize to 0 × 0 must clamp to 1 × 1 and rendering must still succeed. Pinned in Task 6 (`zero_size_is_clamped_to_one_pixel`).
5. **Pointer on the far edge of a device face during port placement.** It must map to the last row and column, not one past it. Pinned in Task 2 (`pointer_on_the_far_edge_of_the_face_uses_the_last_cell`).

## Decisions made in this plan (not fixed by the spec)

- Dimensions: rack depth 800 mm, device depth 450 mm, plinth 60 mm, header panel 100 mm, 40 mm gap between racks, port markers protrude 6 mm.
- The device name is centred on its front face, at most 0.6 U tall. Port markers protrude in front of the face, so on a crowded face they draw over the name.
- A port marker is a square in the upper part of its cell. Its name appears under it (on hover or selection).
- U numbers are printed on the left post of each rack.
- Text is drawn with the browser's 2D canvas on an `OffscreenCanvas`, at 4 px per mm.

---

## File Structure

```
crates/core/Cargo.toml               + wasm-only `uuid/js` feature
crates/render/Cargo.toml
crates/render/src/lib.rs             module list
crates/render/src/layout.rs          world geometry of racks, devices, ports, labels
crates/render/src/pick.rs            Ray, picking, device/port drop targets
crates/render/src/camera.rs          OrbitCamera, framing, Tween
crates/render/src/scene.rs           Scene/SceneInput/Ghost, build_scene, Motion
crates/render/src/view.rs            ViewState: camera + tween + port mode + motion
crates/render/src/text.rs            TextRasterizer trait, label sizes, CanvasTextRasterizer
crates/render/src/gpu/mod.rs         Renderer (wgpu)
crates/render/src/gpu/shaders.wgsl   box, mask, label and outline shaders
crates/render/tests/common/mod.rs    document builders
crates/render/tests/*.rs             one test file per module (+ gpu pixel tests)
```

---

### Task 1: `render` crate, WebAssembly build fix for `core`, and world layout

**Files:**
- Modify: `Cargo.toml` (workspace members), `crates/core/Cargo.toml` (wasm randomness for `uuid`)
- Create: `crates/render/Cargo.toml`, `crates/render/src/lib.rs`, `crates/render/src/layout.rs`
- Test: `crates/render/tests/common/mod.rs`, `crates/render/tests/layout.rs`

**Interfaces:**
- Consumes: `tsv_core::model::{Document, Rack, Device, DEFAULT_RACK_HEIGHT_U}`, `tsv_core::limits::Limits`, `tsv_core::port_grid::Cell`, `tsv_core::ids::RackId`, `tsv_core::name::Name`.
- Produces (`tsv_render::layout`):
  - Constants (mm): `U_MM`, `FRONT_WIDTH_MM`, `RACK_WIDTH_MM`, `POST_WIDTH_MM`, `RACK_DEPTH_MM`, `DEVICE_DEPTH_MM`, `RACK_GAP_MM`, `PLINTH_MM`, `HEADER_MM`, `PANEL_MM`, `PORT_PROTRUSION_MM`, `LABEL_OFFSET_MM`.
  - `Aabb { pub min: Vec3, pub max: Vec3 }` with `new`, `center`, `size`, `union`, `translated(Vec3)`.
  - `FaceRect { pub min: Vec2, pub max: Vec2, pub z: f32 }` with `center() -> Vec3`, `size() -> Vec2`, `contains(Vec2)`, `translated(Vec3)`.
  - `rack_left_x(usize) -> f32`, `u_bottom_y(u32) -> f32`, `rack_total_height(&Rack) -> f32`, `rack_parts(usize, &Rack) -> Vec<Aabb>` (5 parts), `rack_name_rect(usize, &Rack) -> FaceRect`, `u_label_rect(usize, u32) -> FaceRect`.
  - `device_box(usize, &Device) -> Aabb`, `device_face(usize, &Device) -> FaceRect`, `device_name_rect(usize, &Device) -> FaceRect`.
  - `port_cell_rect(usize, &Device, Cell, &Limits) -> FaceRect`, `port_marker_box(usize, &Device, Cell, &Limits) -> Aabb`, `port_label_rect(usize, &Device, Cell, &Limits) -> FaceRect`.
  - `scene_bounds(&Document) -> Aabb`.
  - The first `usize` argument is always the rack's index in `Document::racks`.

- [ ] **Step 1: Create the crate and fix `core` for WebAssembly**

In the root `Cargo.toml`, change the members line to:
```toml
members = ["crates/core", "crates/render"]
```

Append to `crates/core/Cargo.toml` (without it, `uuid` refuses to compile for `wasm32-unknown-unknown` because it has no source of randomness):
```toml

[target.'cfg(all(target_arch = "wasm32", target_os = "unknown"))'.dependencies]
uuid = { version = "1", features = ["js"] }
```

`crates/render/Cargo.toml`:
```toml
[package]
name = "tsv-render"
version.workspace = true
edition.workspace = true

[dependencies]
tsv-core = { path = "../core" }
bytemuck = { version = "1", features = ["derive"] }
glam = "0.34"
wgpu = { version = "30", default-features = false, features = ["std", "wgsl", "webgpu", "vulkan"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = [
    "HtmlCanvasElement",
    "ImageData",
    "OffscreenCanvas",
    "OffscreenCanvasRenderingContext2d",
    "TextMetrics",
] }

[dev-dependencies]
pollster = "1"
```

`crates/render/src/lib.rs` (empty for now, so the crate compiles):
```rust
```

Install the WebAssembly target once per machine:
```bash
rustup target add wasm32-unknown-unknown
```

- [ ] **Step 2: Write the failing tests**

`crates/render/tests/common/mod.rs`:
```rust
#![allow(dead_code)]

use tsv_core::ids::{DeviceId, PortId, RackId};
use tsv_core::limits::Limits;
use tsv_core::model::{Device, DeviceKind, Document, Port, PortKind, Rack, Rgb};
use tsv_core::name::{DocumentName, Name};

pub fn limits() -> Limits {
    Limits::default()
}

pub fn name(s: &str) -> Name {
    Name::parse(s, &limits()).unwrap()
}

pub fn port(n: &str, row: u32, col: u32) -> Port {
    Port {
        id: PortId::new(),
        name: name(n),
        row,
        col,
        kind: PortKind::default(),
    }
}

pub fn device(n: &str, bottom_u: u32, height_u: u32) -> Device {
    Device {
        id: DeviceId::new(),
        name: name(n),
        bottom_u,
        height_u,
        color: Rgb::NEUTRAL_GREY,
        kind: DeviceKind::default(),
        ports: Vec::new(),
    }
}

pub fn with_ports(mut d: Device, ports: Vec<Port>) -> Device {
    d.ports = ports;
    d
}

pub fn rack(n: &str, height_u: u32, devices: Vec<Device>) -> Rack {
    Rack {
        id: RackId::new(),
        name: name(n),
        height_u,
        devices,
    }
}

pub fn doc(racks: Vec<Rack>) -> Document {
    Document {
        name: DocumentName::parse("Test").unwrap(),
        racks,
    }
}

pub fn assert_close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "{a} != {b}");
}
```

`crates/render/tests/layout.rs`:
```rust
mod common;

use common::*;
use glam::{Vec2, Vec3};
use tsv_core::model::Document;
use tsv_core::port_grid::Cell;
use tsv_render::layout::*;

#[test]
fn racks_stand_side_by_side_with_a_gap() {
    assert_close(rack_left_x(0), 0.0);
    assert_close(rack_left_x(2), 2.0 * (RACK_WIDTH_MM + RACK_GAP_MM));
}

#[test]
fn u1_sits_on_the_plinth() {
    assert_close(u_bottom_y(1), PLINTH_MM);
    assert_close(u_bottom_y(3), PLINTH_MM + 2.0 * U_MM);
}

#[test]
fn device_box_spans_its_units_between_the_posts() {
    let d = device("PSU", 3, 2);
    let b = device_box(1, &d);
    let left = rack_left_x(1) + POST_WIDTH_MM;
    assert_close(b.min.x, left);
    assert_close(b.max.x, left + FRONT_WIDTH_MM);
    assert_close(b.min.y, u_bottom_y(3));
    assert_close(b.max.y, u_bottom_y(5));
    assert_close(b.max.z, 0.0);
    assert_close(b.min.z, -DEVICE_DEPTH_MM);
}

#[test]
fn rack_parts_cover_the_full_height_including_header() {
    let r = rack("R", 42, vec![]);
    let bounds = rack_parts(0, &r)
        .into_iter()
        .reduce(|a, b| a.union(&b))
        .unwrap();
    assert_close(bounds.min.y, 0.0);
    assert_close(bounds.max.y, PLINTH_MM + 42.0 * U_MM + HEADER_MM);
    assert_close(bounds.size().x, RACK_WIDTH_MM);
}

#[test]
fn rack_name_sits_on_the_header_above_the_top_unit() {
    let r = rack("R", 10, vec![]);
    let rect = rack_name_rect(0, &r);
    assert!(rect.min.y >= u_bottom_y(11));
    assert!(rect.max.y <= rack_total_height(&r));
    assert!(rect.z > 0.0);
}

#[test]
fn port_cells_divide_the_face_into_a_grid() {
    let d = device("D", 1, 2);
    let w = FRONT_WIDTH_MM / 5.0;
    let face = device_face(0, &d);
    let c = port_cell_rect(0, &d, Cell { row: 1, col: 4 }, &limits());
    assert_close(c.min.x, face.min.x + 4.0 * w);
    assert_close(c.max.x, face.max.x);
    assert_close(c.min.y, face.min.y + U_MM);
    assert_close(c.max.y, face.max.y);
}

#[test]
fn port_marker_protrudes_from_its_cell() {
    let d = device("D", 1, 1);
    let cell = Cell { row: 0, col: 2 };
    let rect = port_cell_rect(0, &d, cell, &limits());
    let marker = port_marker_box(0, &d, cell, &limits());
    assert!(rect.contains(Vec2::new(marker.min.x, marker.min.y)));
    assert!(rect.contains(Vec2::new(marker.max.x, marker.max.y)));
    assert_close(marker.max.z, PORT_PROTRUSION_MM);
    let label = port_label_rect(0, &d, cell, &limits());
    assert!(label.max.y <= marker.min.y, "label sits under the marker");
}

#[test]
fn scene_bounds_cover_all_racks_or_a_default_rack() {
    let d = doc(vec![rack("A", 42, vec![]), rack("B", 20, vec![])]);
    let b = scene_bounds(&d);
    assert_close(b.min.x, 0.0);
    assert_close(b.max.x, rack_left_x(1) + RACK_WIDTH_MM);
    let empty = scene_bounds(&Document { racks: vec![], ..d });
    assert_close(empty.max.x, RACK_WIDTH_MM);
    assert_close(empty.max.y, PLINTH_MM + 42.0 * U_MM + HEADER_MM);
}

#[test]
fn aabb_helpers() {
    let a = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 4.0, 6.0));
    assert_eq!(a.center(), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(a.translated(Vec3::X).min, Vec3::X);
    let u = a.union(&Aabb::new(Vec3::splat(-1.0), Vec3::ONE));
    assert_eq!(
        (u.min, u.max),
        (Vec3::splat(-1.0), Vec3::new(2.0, 4.0, 6.0))
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p tsv-render --test layout`
Expected: compile error, `unresolved import tsv_render::layout`.

- [ ] **Step 4: Implement `layout.rs`**

Add to `crates/render/src/lib.rs`:
```rust
pub mod layout;
```

`crates/render/src/layout.rs`:
```rust
//! World-space geometry of racks, devices and ports (spec §4.2 "World units").
//!
//! Coordinates are millimetres, right-handed, Y up. Racks stand in a row along +X, their front
//! plane is z = 0 and they extend towards -Z. U1 sits directly on top of the plinth.

use glam::{Vec2, Vec3};
use tsv_core::limits::Limits;
use tsv_core::model::{Device, Document, Rack};
use tsv_core::port_grid::Cell;

pub const U_MM: f32 = 44.45;
pub const FRONT_WIDTH_MM: f32 = 482.6;
pub const RACK_WIDTH_MM: f32 = 600.0;
pub const POST_WIDTH_MM: f32 = (RACK_WIDTH_MM - FRONT_WIDTH_MM) / 2.0;
pub const RACK_DEPTH_MM: f32 = 800.0;
pub const DEVICE_DEPTH_MM: f32 = 450.0;
pub const RACK_GAP_MM: f32 = 40.0;
pub const PLINTH_MM: f32 = 60.0;
pub const HEADER_MM: f32 = 100.0;
pub const PANEL_MM: f32 = 20.0;
pub const PORT_PROTRUSION_MM: f32 = 6.0;
/// Labels float this far in front of the surface they are printed on.
pub const LABEL_OFFSET_MM: f32 = 1.0;

/// Axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Aabb {
        Aabb { min, max }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn union(&self, other: &Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn translated(&self, offset: Vec3) -> Aabb {
        Aabb {
            min: self.min + offset,
            max: self.max + offset,
        }
    }
}

/// A rectangle in a plane of constant z, facing +Z (where labels and port grids live).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceRect {
    pub min: Vec2,
    pub max: Vec2,
    pub z: f32,
}

impl FaceRect {
    pub fn center(&self) -> Vec3 {
        let c = (self.min + self.max) * 0.5;
        Vec3::new(c.x, c.y, self.z)
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn translated(&self, offset: Vec3) -> FaceRect {
        FaceRect {
            min: self.min + offset.truncate(),
            max: self.max + offset.truncate(),
            z: self.z + offset.z,
        }
    }
}

pub fn rack_left_x(index: usize) -> f32 {
    index as f32 * (RACK_WIDTH_MM + RACK_GAP_MM)
}

/// Bottom edge of rack unit `u` (U1 is the lowest).
pub fn u_bottom_y(u: u32) -> f32 {
    PLINTH_MM + (u.saturating_sub(1)) as f32 * U_MM
}

pub fn rack_total_height(rack: &Rack) -> f32 {
    PLINTH_MM + rack.height_u as f32 * U_MM + HEADER_MM
}

/// Frame parts of a rack: two side posts, plinth, header panel, top cover.
pub fn rack_parts(index: usize, rack: &Rack) -> Vec<Aabb> {
    let left = rack_left_x(index);
    let right = left + RACK_WIDTH_MM;
    let inner_left = left + POST_WIDTH_MM;
    let inner_right = right - POST_WIDTH_MM;
    let total = rack_total_height(rack);
    let units_top = u_bottom_y(rack.height_u + 1);
    vec![
        Aabb::new(
            Vec3::new(left, 0.0, -RACK_DEPTH_MM),
            Vec3::new(inner_left, total, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_right, 0.0, -RACK_DEPTH_MM),
            Vec3::new(right, total, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_left, 0.0, -RACK_DEPTH_MM),
            Vec3::new(inner_right, PLINTH_MM, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_left, units_top, -PANEL_MM),
            Vec3::new(inner_right, total, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_left, total - PANEL_MM, -RACK_DEPTH_MM),
            Vec3::new(inner_right, total, 0.0),
        ),
    ]
}

/// The header panel's front, where the rack name is printed.
pub fn rack_name_rect(index: usize, rack: &Rack) -> FaceRect {
    let inner_left = rack_left_x(index) + POST_WIDTH_MM;
    let units_top = u_bottom_y(rack.height_u + 1);
    FaceRect {
        min: Vec2::new(inner_left + 10.0, units_top + 15.0),
        max: Vec2::new(
            inner_left + FRONT_WIDTH_MM - 10.0,
            units_top + HEADER_MM - 15.0,
        ),
        z: LABEL_OFFSET_MM,
    }
}

/// U number label on the front of the left post.
pub fn u_label_rect(index: usize, u: u32) -> FaceRect {
    let left = rack_left_x(index);
    let bottom = u_bottom_y(u);
    FaceRect {
        min: Vec2::new(left + 8.0, bottom + U_MM * 0.2),
        max: Vec2::new(left + POST_WIDTH_MM - 8.0, bottom + U_MM * 0.8),
        z: LABEL_OFFSET_MM,
    }
}

pub fn device_box(rack_index: usize, device: &Device) -> Aabb {
    let face = device_face(rack_index, device);
    Aabb::new(
        Vec3::new(face.min.x, face.min.y, -DEVICE_DEPTH_MM),
        Vec3::new(face.max.x, face.max.y, 0.0),
    )
}

/// The device's front face (z = 0).
pub fn device_face(rack_index: usize, device: &Device) -> FaceRect {
    let x = rack_left_x(rack_index) + POST_WIDTH_MM;
    let y = u_bottom_y(device.bottom_u);
    FaceRect {
        min: Vec2::new(x, y),
        max: Vec2::new(x + FRONT_WIDTH_MM, y + device.height_u as f32 * U_MM),
        z: 0.0,
    }
}

/// Where the device name is printed: centred on the face, at most one U tall.
pub fn device_name_rect(rack_index: usize, device: &Device) -> FaceRect {
    let face = device_face(rack_index, device);
    let height = U_MM * 0.6;
    let cy = (face.min.y + face.max.y) * 0.5;
    FaceRect {
        min: Vec2::new(face.min.x + 10.0, cy - height * 0.5),
        max: Vec2::new(face.max.x - 10.0, cy + height * 0.5),
        z: LABEL_OFFSET_MM,
    }
}

/// One cell of the device's port grid (row 0 at the bottom).
pub fn port_cell_rect(rack_index: usize, device: &Device, cell: Cell, limits: &Limits) -> FaceRect {
    let face = device_face(rack_index, device);
    let rows = (device.height_u * limits.port_rows_per_u).max(1) as f32;
    let w = FRONT_WIDTH_MM / limits.port_cols.max(1) as f32;
    let h = face.size().y / rows;
    let min = Vec2::new(
        face.min.x + cell.col as f32 * w,
        face.min.y + cell.row as f32 * h,
    );
    FaceRect {
        min,
        max: min + Vec2::new(w, h),
        z: 0.0,
    }
}

/// The small square marker drawn for a port, in the upper part of its cell.
pub fn port_marker_box(rack_index: usize, device: &Device, cell: Cell, limits: &Limits) -> Aabb {
    let r = port_cell_rect(rack_index, device, cell, limits);
    let size = r.size();
    let side = size.x.min(size.y) * 0.45;
    let cx = (r.min.x + r.max.x) * 0.5;
    let cy = r.min.y + size.y * 0.62;
    Aabb::new(
        Vec3::new(cx - side * 0.5, cy - side * 0.5, 0.0),
        Vec3::new(cx + side * 0.5, cy + side * 0.5, PORT_PROTRUSION_MM),
    )
}

/// Where a port's name appears (under its marker) while hovered or selected.
pub fn port_label_rect(
    rack_index: usize,
    device: &Device,
    cell: Cell,
    limits: &Limits,
) -> FaceRect {
    let r = port_cell_rect(rack_index, device, cell, limits);
    let size = r.size();
    FaceRect {
        min: Vec2::new(r.min.x + size.x * 0.05, r.min.y + size.y * 0.06),
        max: Vec2::new(r.max.x - size.x * 0.05, r.min.y + size.y * 0.34),
        z: LABEL_OFFSET_MM,
    }
}

/// Bounds of everything in the document; a default 42U rack's bounds when it has no racks.
pub fn scene_bounds(doc: &Document) -> Aabb {
    let default_rack;
    let racks: Vec<(usize, &Rack)> = if doc.racks.is_empty() {
        default_rack = Rack {
            id: tsv_core::ids::RackId::new(),
            name: tsv_core::name::Name::parse("R", &Limits::default()).expect("valid"),
            height_u: tsv_core::model::DEFAULT_RACK_HEIGHT_U,
            devices: Vec::new(),
        };
        vec![(0, &default_rack)]
    } else {
        doc.racks.iter().enumerate().collect()
    };
    racks
        .into_iter()
        .flat_map(|(i, r)| rack_parts(i, r))
        .reduce(|a, b| a.union(&b))
        .expect("at least one rack part")
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test layout`
Expected: all 9 tests PASS.

- [ ] **Step 6: Check the WebAssembly build**

Run: `cargo check -p tsv-render --target wasm32-unknown-unknown`
Expected: `Finished` with no errors (this proves the `uuid` fix: `tsv-core` now builds for the browser).

Run: `cargo test -p tsv-core`
Expected: all 115 `core` tests still PASS.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/core/Cargo.toml crates/render
git commit -m "feat(render): add render crate with world layout

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```


---

### Task 2: Rays and picking (`pick`)

**Files:**
- Create: `crates/render/src/pick.rs`
- Modify: `crates/render/src/lib.rs`
- Test: `crates/render/tests/pick.rs`

**Interfaces:**
- Consumes: `layout` (Task 1); `tsv_core::edit::ObjectId`, `tsv_core::port_grid::{Cell, PushDir, push_direction}`.
- Produces (`tsv_render::pick`):
  - `Ray { pub origin: Vec3, pub dir: Vec3 }` (dir unit length) with `hit_aabb(&Aabb) -> Option<f32>` and `hit_plane_z(f32) -> Option<Vec3>`.
  - `pick(&Document, &Limits, &Ray) -> Option<ObjectId>`: the nearest rack part, device or port marker.
  - `grab_offset_u(&Device, y: f32) -> u32`: which U of the device (0 = bottom) is under world height `y`, clamped to the device.
  - `DeviceTarget { pub rack: RackId, pub bottom_u: u32 }` and `device_drop_target(&Document, &Ray, grab_offset_u: u32) -> Option<DeviceTarget>`. `bottom_u` is ≥ 1 but otherwise unclamped (pass it straight to `tsv_core::edit::plan_device_drop`, which clamps).
  - `PortTarget { pub cell: Cell, pub dir: PushDir }` and `port_drop_target(&Document, &Limits, DeviceId, &Ray) -> Option<PortTarget>` (`None` when the ray misses that device's face).

- [ ] **Step 1: Write the failing tests**

`crates/render/tests/pick.rs`:
```rust
mod common;

use common::*;
use glam::Vec3;
use tsv_core::edit::ObjectId;
use tsv_core::port_grid::{Cell, PushDir};
use tsv_render::layout::*;
use tsv_render::pick::*;

/// A ray straight into the scene (towards -Z) through world point (x, y).
fn ray_at(x: f32, y: f32) -> Ray {
    Ray {
        origin: Vec3::new(x, y, 5000.0),
        dir: Vec3::NEG_Z,
    }
}

#[test]
fn ray_box_intersection() {
    let b = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(ray_at(0.0, 0.0).hit_aabb(&b), Some(4999.0));
    assert_eq!(ray_at(5.0, 0.0).hit_aabb(&b), None);
    let behind = Ray {
        origin: Vec3::new(0.0, 0.0, -10.0),
        dir: Vec3::NEG_Z,
    };
    assert_eq!(behind.hit_aabb(&b), None);
}

#[test]
fn ray_plane_intersection() {
    assert_eq!(
        ray_at(3.0, 4.0).hit_plane_z(0.0),
        Some(Vec3::new(3.0, 4.0, 0.0))
    );
    let parallel = Ray {
        origin: Vec3::ZERO,
        dir: Vec3::X,
    };
    assert_eq!(parallel.hit_plane_z(0.0), None);
}

#[test]
fn picks_the_nearest_object() {
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let d = with_ports(device("D", 5, 1), vec![p]);
    let did = d.id;
    let r = rack("R", 42, vec![d]);
    let rid = r.id;
    let doc = doc(vec![r]);
    let dev = &doc.racks[0].devices[0];

    let marker = port_marker_box(0, dev, Cell { row: 0, col: 0 }, &limits()).center();
    assert_eq!(
        pick(&doc, &limits(), &ray_at(marker.x, marker.y)),
        Some(ObjectId::Port(pid))
    );
    let face = device_face(0, dev).center();
    assert_eq!(
        pick(&doc, &limits(), &ray_at(face.x + 150.0, face.y)),
        Some(ObjectId::Device(did))
    );
    assert_eq!(
        pick(&doc, &limits(), &ray_at(10.0, 500.0)),
        Some(ObjectId::Rack(rid))
    );
    assert_eq!(pick(&doc, &limits(), &ray_at(-500.0, 500.0)), None);
}

#[test]
fn grab_offset_is_the_unit_under_the_pointer() {
    let d = device("D", 10, 3);
    assert_eq!(grab_offset_u(&d, u_bottom_y(10) + 1.0), 0);
    assert_eq!(grab_offset_u(&d, u_bottom_y(12) + 1.0), 2);
    assert_eq!(
        grab_offset_u(&d, u_bottom_y(30)),
        2,
        "clamped to the device"
    );
    assert_eq!(grab_offset_u(&d, 0.0), 0, "clamped to the device");
}

#[test]
fn device_drop_target_snaps_to_units_and_racks() {
    let doc = doc(vec![rack("A", 42, vec![]), rack("B", 42, vec![])]);
    let x_b = rack_left_x(1) + 300.0;
    let t = device_drop_target(&doc, &ray_at(x_b, u_bottom_y(12) + 5.0), 0).unwrap();
    assert_eq!(
        t,
        DeviceTarget {
            rack: doc.racks[1].id,
            bottom_u: 12
        }
    );
    let t = device_drop_target(&doc, &ray_at(x_b, u_bottom_y(12) + 5.0), 1).unwrap();
    assert_eq!(t.bottom_u, 11, "grabbed one unit above its bottom");
    let t = device_drop_target(&doc, &ray_at(x_b, 1.0), 2).unwrap();
    assert_eq!(t.bottom_u, 1, "never below U1");
    assert_eq!(
        device_drop_target(&doc, &ray_at(rack_left_x(1) - 20.0, 500.0), 0),
        None,
        "in the gap"
    );
}

#[test]
fn port_drop_target_finds_cell_and_push_direction() {
    let d = device("D", 3, 2);
    let did = d.id;
    let doc = doc(vec![rack("R", 42, vec![d])]);
    let dev = &doc.racks[0].devices[0];
    let cell = Cell { row: 1, col: 3 };
    let c = port_cell_rect(0, dev, cell, &limits()).center();
    let w = FRONT_WIDTH_MM / 5.0;

    let left = port_drop_target(&doc, &limits(), did, &ray_at(c.x - w * 0.3, c.y)).unwrap();
    assert_eq!(
        left,
        PortTarget {
            cell,
            dir: PushDir::Right
        }
    );
    let below = port_drop_target(&doc, &limits(), did, &ray_at(c.x, c.y - U_MM * 0.3)).unwrap();
    assert_eq!(
        below,
        PortTarget {
            cell,
            dir: PushDir::Up
        }
    );
    let face = device_face(0, dev);
    assert_eq!(
        port_drop_target(&doc, &limits(), did, &ray_at(face.max.x + 5.0, c.y)),
        None
    );
}

#[test]
fn pointer_exactly_on_a_box_edge_still_picks_cleanly() {
    let b = Aabb::new(Vec3::ZERO, Vec3::splat(10.0));
    assert_eq!(ray_at(0.0, 5.0).hit_aabb(&b), Some(4990.0));
    assert_eq!(ray_at(10.0, 10.0).hit_aabb(&b), Some(4990.0));
}

#[test]
fn empty_document_picks_nothing() {
    let doc = doc(vec![]);
    assert_eq!(pick(&doc, &limits(), &ray_at(100.0, 100.0)), None);
    assert_eq!(device_drop_target(&doc, &ray_at(100.0, 100.0), 0), None);
}

#[test]
fn pointer_on_the_far_edge_of_the_face_uses_the_last_cell() {
    let d = device("D", 1, 1);
    let did = d.id;
    let doc = doc(vec![rack("R", 42, vec![d])]);
    let face = device_face(0, &doc.racks[0].devices[0]);
    let t = port_drop_target(&doc, &limits(), did, &ray_at(face.max.x, face.max.y)).unwrap();
    assert_eq!(t.cell, Cell { row: 0, col: 4 });
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p tsv-render --test pick`
Expected: compile error, `unresolved import tsv_render::pick`.

- [ ] **Step 3: Implement `pick.rs`**

Add to `crates/render/src/lib.rs`:
```rust
pub mod pick;
```

`crates/render/src/pick.rs`:
```rust
//! CPU picking and drag targeting (spec §4.2 "Picking").

use glam::{Vec2, Vec3};
use tsv_core::edit::ObjectId;
use tsv_core::ids::{DeviceId, RackId};
use tsv_core::limits::Limits;
use tsv_core::model::{Device, Document};
use tsv_core::port_grid::{Cell, PushDir, push_direction};

use crate::layout::{
    Aabb, PLINTH_MM, RACK_WIDTH_MM, U_MM, device_box, device_face, port_cell_rect, port_marker_box,
    rack_left_x, rack_parts,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    /// Unit length.
    pub dir: Vec3,
}

impl Ray {
    /// Distance along the ray to the box's entry point (0 if the origin is inside).
    pub fn hit_aabb(&self, b: &Aabb) -> Option<f32> {
        let mut t_near = 0.0f32;
        let mut t_far = f32::INFINITY;
        for axis in 0..3 {
            let (o, d) = (self.origin[axis], self.dir[axis]);
            let (lo, hi) = (b.min[axis], b.max[axis]);
            if d == 0.0 {
                // Parallel to this slab: inside it or never (avoids 0 × ∞ = NaN on edges).
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let (t1, t2) = ((lo - o) / d, (hi - o) / d);
            t_near = t_near.max(t1.min(t2));
            t_far = t_far.min(t1.max(t2));
        }
        (t_far >= t_near).then_some(t_near)
    }

    /// Point where the ray crosses the plane of constant `z`, if it does (in front of the origin).
    pub fn hit_plane_z(&self, z: f32) -> Option<Vec3> {
        if self.dir.z.abs() < 1e-6 {
            return None;
        }
        let t = (z - self.origin.z) / self.dir.z;
        (t >= 0.0).then(|| self.origin + self.dir * t)
    }
}

/// The nearest rack, device or port under the ray.
pub fn pick(doc: &Document, limits: &Limits, ray: &Ray) -> Option<ObjectId> {
    let mut best: Option<(f32, ObjectId)> = None;
    let mut consider = |t: Option<f32>, id: ObjectId| {
        if let Some(t) = t
            && best.is_none_or(|(bt, _)| t < bt)
        {
            best = Some((t, id));
        }
    };
    for (ri, rack) in doc.racks.iter().enumerate() {
        for part in rack_parts(ri, rack) {
            consider(ray.hit_aabb(&part), ObjectId::Rack(rack.id));
        }
        for device in &rack.devices {
            consider(
                ray.hit_aabb(&device_box(ri, device)),
                ObjectId::Device(device.id),
            );
            for port in &device.ports {
                let cell = Cell {
                    row: port.row,
                    col: port.col,
                };
                consider(
                    ray.hit_aabb(&port_marker_box(ri, device, cell, limits)),
                    ObjectId::Port(port.id),
                );
            }
        }
    }
    best.map(|(_, id)| id)
}

/// Which U of `device` lies under world height `y` (0 = its bottom U), clamped to the device.
pub fn grab_offset_u(device: &Device, y: f32) -> u32 {
    let u_under = ((y - PLINTH_MM) / U_MM).floor() as i64 + 1;
    (u_under - i64::from(device.bottom_u)).clamp(0, i64::from(device.height_u) - 1) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceTarget {
    pub rack: RackId,
    /// Snapped, unclamped bottom U (core clamps it into the rack). Never below 1.
    pub bottom_u: u32,
}

/// Where a device dragged by its `grab_offset_u`-th U would land: the rack whose outline the
/// ray crosses on the front plane, and the U under the pointer minus the grab offset.
pub fn device_drop_target(doc: &Document, ray: &Ray, grab_offset_u: u32) -> Option<DeviceTarget> {
    let p = ray.hit_plane_z(0.0)?;
    let rack_index = (0..doc.racks.len()).find(|&i| {
        let left = rack_left_x(i);
        p.x >= left && p.x <= left + RACK_WIDTH_MM
    })?;
    let u_under = ((p.y - PLINTH_MM) / U_MM).floor() as i64 + 1;
    let bottom = (u_under - i64::from(grab_offset_u)).clamp(1, i64::from(u32::MAX)) as u32;
    Some(DeviceTarget {
        rack: doc.racks[rack_index].id,
        bottom_u: bottom,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortTarget {
    pub cell: Cell,
    pub dir: PushDir,
}

/// The cell of `device`'s face under the ray, and the push direction from the pointer's offset
/// within that cell. `None` when the ray misses the face.
pub fn port_drop_target(
    doc: &Document,
    limits: &Limits,
    device_id: DeviceId,
    ray: &Ray,
) -> Option<PortTarget> {
    let (ri, device) = doc.racks.iter().enumerate().find_map(|(ri, r)| {
        r.devices
            .iter()
            .find(|d| d.id == device_id)
            .map(|d| (ri, d))
    })?;
    let face = device_face(ri, device);
    let p = ray.hit_plane_z(0.0)?;
    let p2 = Vec2::new(p.x, p.y);
    if !face.contains(p2) {
        return None;
    }
    let rows = device.height_u * limits.port_rows_per_u;
    let cell_size = port_cell_rect(ri, device, Cell { row: 0, col: 0 }, limits).size();
    let local = p2 - face.min;
    let col = ((local.x / cell_size.x).floor() as u32).min(limits.port_cols - 1);
    let row = ((local.y / cell_size.y).floor() as u32).min(rows - 1);
    let cell = Cell { row, col };
    let centre = port_cell_rect(ri, device, cell, limits).center();
    let dx = (p.x - centre.x) / cell_size.x;
    let dy = (p.y - centre.y) / cell_size.y;
    Some(PortTarget {
        cell,
        dir: push_direction(dx, dy),
    })
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test pick`
Expected: all 9 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/render
git commit -m "feat(render): add ray picking and drag targeting

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```


---

### Task 3: Orbit camera, framing and tweens (`camera`)

**Files:**
- Create: `crates/render/src/camera.rs`
- Modify: `crates/render/src/lib.rs`
- Test: `crates/render/tests/camera.rs`

**Interfaces:**
- Consumes: `layout::{Aabb, FaceRect}` (Task 1), `pick::Ray` (Task 2).
- Produces (`tsv_render::camera`):
  - Constants: `FOV_Y`, `NEAR_MM`, `FAR_MM`, `MIN_DISTANCE_MM`, `MAX_DISTANCE_MM`, `MAX_PITCH`, `FRONT_VIEW_PITCH` (10°), `ORBIT_SPEED`, `FRAME_MARGIN`, `TWEEN_SECONDS` (0.4).
  - `OrbitCamera { pub target: Vec3, pub yaw: f32, pub pitch: f32, pub distance: f32 }` with `eye`, `view`, `projection(aspect)`, `view_proj(aspect) -> Mat4`, `orbit(dx_px, dy_px)`, `pan(dx_px, dy_px, viewport_height_px)`, `zoom(wheel_delta)`, `framing(&Aabb, aspect, yaw, pitch)`, `front_view(&Aabb, aspect)`, `face_view(&FaceRect, aspect)`, `ray(ndc: Vec2, aspect) -> Ray`.
  - `pixel_to_ndc(px: Vec2, viewport: Vec2) -> Vec2` (pixel origin top-left).
  - `Tween { pub from, pub to, pub start_s: f64 }` with `sample(now_s) -> (OrbitCamera, bool /* done */)`; the final sample is exactly `to`.
  - Projection uses WebGPU conventions (depth 0..1, Y up) via `glam::camera::rh::proj::directx`; glam 0.34 has no `Mat4::perspective_rh`/`look_at_rh`.

- [ ] **Step 1: Write the failing tests**

`crates/render/tests/camera.rs`:
```rust
mod common;

use common::assert_close;
use glam::{Vec2, Vec3};
use tsv_render::camera::*;
use tsv_render::layout::{Aabb, FaceRect};

fn front(distance: f32) -> OrbitCamera {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        distance,
    }
}

#[test]
fn zero_yaw_and_pitch_looks_at_the_rack_fronts() {
    let eye = front(1000.0).eye();
    assert_close(eye.x, 0.0);
    assert_close(eye.y, 0.0);
    assert_close(eye.z, 1000.0);
}

#[test]
fn orbit_clamps_pitch() {
    let mut c = front(1000.0);
    c.orbit(0.0, 1.0e6);
    assert_close(c.pitch, MAX_PITCH);
    c.orbit(0.0, -1.0e6);
    assert_close(c.pitch, -MAX_PITCH);
}

#[test]
fn zoom_is_clamped() {
    let mut c = front(1000.0);
    c.zoom(-1.0e6);
    assert_close(c.distance, MIN_DISTANCE_MM);
    c.zoom(1.0e6);
    assert_close(c.distance, MAX_DISTANCE_MM);
}

#[test]
fn pan_moves_the_target_against_the_pointer() {
    let mut c = front(1000.0);
    c.pan(10.0, 0.0, 500.0);
    assert!(c.target.x < 0.0, "dragging right moves the target left");
    assert_close(c.target.y, 0.0);
}

#[test]
fn centre_ray_points_at_the_target() {
    let c = front(1000.0);
    let ray = c.ray(Vec2::ZERO, 1.5);
    assert_close(ray.dir.z, -1.0);
    let p = ray.hit_plane_z(0.0).unwrap();
    assert_close(p.x, 0.0);
    assert_close(p.y, 0.0);
}

#[test]
fn pixel_to_ndc_maps_corners() {
    let v = Vec2::new(200.0, 100.0);
    assert_eq!(pixel_to_ndc(Vec2::ZERO, v), Vec2::new(-1.0, 1.0));
    assert_eq!(pixel_to_ndc(v, v), Vec2::new(1.0, -1.0));
}

#[test]
fn framing_keeps_the_whole_box_in_view() {
    let b = Aabb::new(Vec3::new(0.0, 0.0, -800.0), Vec3::new(3000.0, 2000.0, 0.0));
    let c = OrbitCamera::front_view(&b, 16.0 / 9.0);
    assert_eq!(c.target, b.center());
    assert_close(c.yaw, 0.0);
    assert_close(c.pitch, FRONT_VIEW_PITCH);
    let vp = c.view_proj(16.0 / 9.0);
    for x in [b.min.x, b.max.x] {
        for y in [b.min.y, b.max.y] {
            for z in [b.min.z, b.max.z] {
                let ndc = vp.project_point3(Vec3::new(x, y, z));
                assert!(
                    ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
                    "{x},{y},{z} → {ndc}"
                );
            }
        }
    }
}

#[test]
fn face_view_looks_straight_at_the_face() {
    let face = FaceRect {
        min: Vec2::new(100.0, 200.0),
        max: Vec2::new(582.6, 288.9),
        z: 0.0,
    };
    let c = OrbitCamera::face_view(&face, 1.0);
    assert_eq!(c.target, face.center());
    assert_close(c.yaw, 0.0);
    assert_close(c.pitch, 0.0);
    let ndc = c
        .view_proj(1.0)
        .project_point3(Vec3::new(582.6, 288.9, 0.0));
    assert!(
        ndc.x <= 1.0 && ndc.x > 0.7,
        "face fills most of the width: {ndc}"
    );
}

#[test]
fn tween_eases_from_start_to_end() {
    let from = front(1000.0);
    let to = OrbitCamera {
        target: Vec3::new(100.0, 0.0, 0.0),
        yaw: 1.0,
        pitch: 0.5,
        distance: 2000.0,
    };
    let tween = Tween {
        from,
        to,
        start_s: 10.0,
    };
    assert_eq!(tween.sample(10.0), (from, false));
    let (mid, done) = tween.sample(10.0 + TWEEN_SECONDS / 2.0);
    assert!(!done);
    assert_close(mid.distance, 1500.0);
    assert_eq!(tween.sample(10.0 + TWEEN_SECONDS), (to, true));
    assert!(tween.sample(99.0).1);
}

#[test]
fn tween_turns_the_short_way_round() {
    let from = OrbitCamera {
        yaw: 3.0,
        ..front(1000.0)
    };
    let to = OrbitCamera {
        yaw: -3.0,
        ..front(1000.0)
    };
    let (mid, _) = Tween {
        from,
        to,
        start_s: 0.0,
    }
    .sample(TWEEN_SECONDS / 2.0);
    assert!(mid.yaw > 3.0, "passes through π, not 0: {}", mid.yaw);
}

#[test]
fn largest_document_fits_the_front_view() {
    use tsv_core::ids::RackId;
    use tsv_core::model::{Document, Rack};
    use tsv_core::name::{DocumentName, Name};
    use tsv_render::layout::scene_bounds;
    let limits = tsv_core::limits::Limits::default();
    let racks = (1..=5)
        .map(|i| Rack {
            id: RackId::new(),
            name: Name::parse(&format!("R{i}"), &limits).unwrap(),
            height_u: limits.max_rack_height_u,
            devices: vec![],
        })
        .collect();
    let doc = Document {
        name: DocumentName::parse("Big").unwrap(),
        racks,
    };
    let b = scene_bounds(&doc);
    let c = OrbitCamera::front_view(&b, 16.0 / 9.0);
    assert!(c.distance < MAX_DISTANCE_MM, "not clamped: {}", c.distance);
    let ndc = c.view_proj(16.0 / 9.0).project_point3(b.max);
    assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p tsv-render --test camera`
Expected: compile error, `unresolved import tsv_render::camera`.

- [ ] **Step 3: Implement `camera.rs`**

Add to `crates/render/src/lib.rs`:
```rust
pub mod camera;
```

`crates/render/src/camera.rs`:
```rust
//! Orbit camera, framing and camera tweens (spec §4.2 "Camera").

use std::f32::consts::FRAC_PI_2;

use glam::camera::rh::proj::directx;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec2, Vec3};

use crate::layout::{Aabb, FaceRect};
use crate::pick::Ray;

pub const FOV_Y: f32 = 45.0 * std::f32::consts::PI / 180.0;
pub const NEAR_MM: f32 = 50.0;
pub const FAR_MM: f32 = 200_000.0;
pub const MIN_DISTANCE_MM: f32 = 150.0;
pub const MAX_DISTANCE_MM: f32 = 60_000.0;
pub const MAX_PITCH: f32 = 85.0 * std::f32::consts::PI / 180.0;
/// Pitch of the default front view: slightly from above.
pub const FRONT_VIEW_PITCH: f32 = 10.0 * std::f32::consts::PI / 180.0;
/// Radians per pixel of pointer movement while orbiting.
pub const ORBIT_SPEED: f32 = 0.005;
/// Margin around framed content (1.0 = tight).
pub const FRAME_MARGIN: f32 = 1.15;
pub const TWEEN_SECONDS: f64 = 0.4;

/// Camera orbiting `target`. `yaw = 0, pitch = 0` looks straight at the rack fronts (towards -Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let dir = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        self.target + dir * self.distance
    }

    pub fn view(&self) -> Mat4 {
        look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    pub fn projection(aspect: f32) -> Mat4 {
        directx::perspective(FOV_Y, aspect.max(0.01), NEAR_MM, FAR_MM)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        Self::projection(aspect) * self.view()
    }

    /// Rotates around the target; `dx`/`dy` are pointer deltas in pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * ORBIT_SPEED;
        self.pitch = (self.pitch + dy * ORBIT_SPEED).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Moves the target so the scene follows the pointer; deltas in pixels.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height_px: f32) {
        let mm_per_px = 2.0 * self.distance * (FOV_Y * 0.5).tan() / viewport_height_px.max(1.0);
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        self.target += (-dx * right + dy * up) * mm_per_px;
    }

    /// Zooms by a wheel delta (positive = away).
    pub fn zoom(&mut self, wheel_delta: f32) {
        self.distance =
            (self.distance * 1.001f32.powf(wheel_delta)).clamp(MIN_DISTANCE_MM, MAX_DISTANCE_MM);
    }

    /// A camera at `yaw`/`pitch` that fits `bounds` in the viewport.
    pub fn framing(bounds: &Aabb, aspect: f32, yaw: f32, pitch: f32) -> OrbitCamera {
        let radius = bounds.size().length() * 0.5;
        let half_fov_x = ((FOV_Y * 0.5).tan() * aspect.max(0.01)).atan();
        let half_fov = (FOV_Y * 0.5).min(half_fov_x);
        let distance =
            (radius * FRAME_MARGIN / half_fov.sin()).clamp(MIN_DISTANCE_MM, MAX_DISTANCE_MM);
        OrbitCamera {
            target: bounds.center(),
            yaw,
            pitch,
            distance,
        }
    }

    /// The "Reset view" camera: all of `bounds` seen from the front, slightly from above.
    pub fn front_view(bounds: &Aabb, aspect: f32) -> OrbitCamera {
        Self::framing(bounds, aspect, 0.0, FRONT_VIEW_PITCH)
    }

    /// Straight-on view filling the viewport with `face` (port placement mode).
    pub fn face_view(face: &FaceRect, aspect: f32) -> OrbitCamera {
        let size = face.size() * FRAME_MARGIN;
        let tan_half = (FOV_Y * 0.5).tan();
        let for_height = size.y * 0.5 / tan_half;
        let for_width = size.x * 0.5 / (tan_half * aspect.max(0.01));
        OrbitCamera {
            target: face.center(),
            yaw: 0.0,
            pitch: 0.0,
            distance: for_height
                .max(for_width)
                .clamp(MIN_DISTANCE_MM, MAX_DISTANCE_MM),
        }
    }

    /// World-space ray through a point given in normalized device coordinates (-1..1, +y up).
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> Ray {
        let inverse = self.view_proj(aspect).inverse();
        let near = inverse.project_point3(ndc.extend(0.0));
        let far = inverse.project_point3(ndc.extend(1.0));
        Ray {
            origin: near,
            dir: (far - near).normalize(),
        }
    }
}

/// Converts a pointer position in pixels (origin top-left) to normalized device coordinates.
pub fn pixel_to_ndc(px: Vec2, viewport: Vec2) -> Vec2 {
    Vec2::new(px.x / viewport.x * 2.0 - 1.0, 1.0 - px.y / viewport.y * 2.0)
}

/// An eased camera move over `TWEEN_SECONDS`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    pub from: OrbitCamera,
    pub to: OrbitCamera,
    pub start_s: f64,
}

impl Tween {
    /// The camera at time `now_s` and whether the tween has finished.
    pub fn sample(&self, now_s: f64) -> (OrbitCamera, bool) {
        let t = ((now_s - self.start_s) / TWEEN_SECONDS).clamp(0.0, 1.0) as f32;
        if t >= 1.0 {
            return (self.to, true);
        }
        let e = t * t * (3.0 - 2.0 * t);
        let lerp = |a: f32, b: f32| a + (b - a) * e;
        let camera = OrbitCamera {
            target: self.from.target.lerp(self.to.target, e),
            yaw: self.from.yaw + shortest_angle(self.from.yaw, self.to.yaw) * e,
            pitch: lerp(self.from.pitch, self.to.pitch),
            distance: lerp(self.from.distance, self.to.distance),
        };
        (camera, false)
    }
}

/// Signed difference `to - from` wrapped into (-π, π].
fn shortest_angle(from: f32, to: f32) -> f32 {
    let tau = 4.0 * FRAC_PI_2;
    let mut d = (to - from) % tau;
    if d > tau / 2.0 {
        d -= tau;
    } else if d <= -tau / 2.0 {
        d += tau;
    }
    d
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test camera`
Expected: all 11 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/render
git commit -m "feat(render): add orbit camera, framing and tweens

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```


---

### Task 4: Draw lists and motion (`scene`)

**Files:**
- Create: `crates/render/src/scene.rs`
- Modify: `crates/render/src/lib.rs`
- Test: `crates/render/tests/scene.rs`

**Interfaces:**
- Consumes: `layout` (Task 1); `tsv_core::edit::ObjectId`, `tsv_core::ids::PortId`, `tsv_core::model::Rgb`.
- Produces (`tsv_render::scene`):
  - Constants: `BACKGROUND`, `FRAME_COLOR`, `GHOST_INVALID`, `GHOST_ALPHA` (0.45), `MOTION_SECONDS` (0.12).
  - `BoxInstance { pub aabb: Aabb, pub color: [f32; 4] }`, `Label { pub text: String, pub rect: FaceRect, pub color: [u8; 4] }`, `Ghost { pub aabb: Aabb, pub color: Rgb, pub valid: bool }`.
  - `SceneInput<'a> { pub document: &'a Document, pub limits: &'a Limits, pub selection: Option<ObjectId>, pub hovered_port: Option<PortId>, pub ghost: Option<Ghost> }`.
  - `Scene { pub opaque, pub translucent, pub selected: Vec<BoxInstance>, pub labels: Vec<Label> }` (`Default`).
  - `rgba(Rgb, alpha) -> [f32; 4]`, `contrast_text(Rgb) -> [u8; 4]`.
  - `animation_targets(&Document, &Limits) -> Vec<(ObjectId, Vec3)>`.
  - `Motion` (`Default`) with `update(&[(ObjectId, Vec3)], dt_s) -> bool /* still moving */`, `snap(&[(ObjectId, Vec3)])`, `offset(ObjectId, target: Vec3) -> Vec3`.
  - `build_scene(&SceneInput, &Motion) -> Scene`.

- [ ] **Step 1: Write the failing tests**

`crates/render/tests/scene.rs`:
```rust
mod common;

use common::*;
use glam::Vec3;
use tsv_core::edit::ObjectId;
use tsv_core::model::Rgb;
use tsv_render::layout::*;
use tsv_render::scene::*;

fn input<'a>(d: &'a tsv_core::model::Document, l: &'a tsv_core::limits::Limits) -> SceneInput<'a> {
    SceneInput {
        document: d,
        limits: l,
        selection: None,
        hovered_port: None,
        ghost: None,
    }
}

fn texts(scene: &Scene) -> Vec<&str> {
    scene.labels.iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn contrast_text_picks_black_or_white() {
    assert_eq!(
        contrast_text(Rgb {
            r: 250,
            g: 250,
            b: 250
        }),
        [0, 0, 0, 255]
    );
    assert_eq!(
        contrast_text(Rgb {
            r: 20,
            g: 20,
            b: 60
        }),
        [255, 255, 255, 255]
    );
}

#[test]
fn racks_devices_and_labels_are_drawn() {
    let mut dmm = device("DMM", 3, 2);
    dmm.color = Rgb { r: 255, g: 0, b: 0 };
    let d = doc(vec![rack("Rack1", 4, vec![dmm.clone()])]);
    let l = limits();
    let scene = build_scene(&input(&d, &l), &Motion::default());
    assert_eq!(scene.opaque.len(), 5 + 1, "five frame parts and one device");
    let device_box_drawn = scene
        .opaque
        .iter()
        .find(|b| b.aabb == device_box(0, &dmm))
        .unwrap();
    assert_eq!(device_box_drawn.color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(texts(&scene), vec!["Rack1", "1", "2", "3", "4", "DMM"]);
    assert!(scene.selected.is_empty() && scene.translucent.is_empty());
}

#[test]
fn port_labels_appear_only_when_hovered_or_selected() {
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let d = doc(vec![rack(
        "R",
        2,
        vec![with_ports(device("D", 1, 1), vec![p])],
    )]);
    let l = limits();
    let plain = build_scene(&input(&d, &l), &Motion::default());
    assert!(!texts(&plain).contains(&"CH1"));
    assert_eq!(plain.opaque.len(), 5 + 1 + 1, "port marker is drawn");
    let hovered = build_scene(
        &SceneInput {
            hovered_port: Some(pid),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert!(texts(&hovered).contains(&"CH1"));
    let selected = build_scene(
        &SceneInput {
            selection: Some(ObjectId::Port(pid)),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert!(texts(&selected).contains(&"CH1"));
    assert_eq!(selected.selected.len(), 1);
}

#[test]
fn selecting_a_rack_outlines_all_its_parts() {
    let d = doc(vec![rack("R", 2, vec![device("D", 1, 1)])]);
    let l = limits();
    let s = build_scene(
        &SceneInput {
            selection: Some(ObjectId::Rack(d.racks[0].id)),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(s.selected.len(), 5);
}

#[test]
fn ghost_is_translucent_and_red_when_invalid() {
    let d = doc(vec![]);
    let l = limits();
    let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
    let color = Rgb { r: 0, g: 0, b: 255 };
    let valid = build_scene(
        &SceneInput {
            ghost: Some(Ghost {
                aabb,
                color,
                valid: true,
            }),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(
        valid.translucent,
        vec![BoxInstance {
            aabb,
            color: rgba(color, GHOST_ALPHA)
        }]
    );
    let invalid = build_scene(
        &SceneInput {
            ghost: Some(Ghost {
                aabb,
                color,
                valid: false,
            }),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(
        invalid.translucent[0].color,
        rgba(GHOST_INVALID, GHOST_ALPHA)
    );
}

#[test]
fn motion_eases_towards_new_positions() {
    let id = ObjectId::Device(tsv_core::ids::DeviceId::new());
    let mut m = Motion::default();
    assert!(
        !m.update(&[(id, Vec3::ZERO)], 0.016),
        "new objects appear in place"
    );
    assert!(m.update(&[(id, Vec3::new(100.0, 0.0, 0.0))], 0.016));
    let offset = m.offset(id, Vec3::new(100.0, 0.0, 0.0));
    assert!(offset.x < 0.0 && offset.x > -100.0, "part-way: {offset}");
    for _ in 0..60 {
        m.update(&[(id, Vec3::new(100.0, 0.0, 0.0))], 0.016);
    }
    assert_eq!(m.offset(id, Vec3::new(100.0, 0.0, 0.0)), Vec3::ZERO);
}

#[test]
fn moving_devices_are_drawn_at_their_displayed_position() {
    let dev = device("D", 1, 1);
    let id = ObjectId::Device(dev.id);
    let d = doc(vec![rack("R", 10, vec![dev.clone()])]);
    let l = limits();
    let mut m = Motion::default();
    m.snap(&[(id, device_box(0, &dev).min - Vec3::new(0.0, 10.0, 0.0))]);
    let s = build_scene(&input(&d, &l), &m);
    let drawn = s.opaque.last().unwrap();
    assert_close(drawn.aabb.min.y, device_box(0, &dev).min.y - 10.0);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p tsv-render --test scene`
Expected: compile error, `unresolved import tsv_render::scene`.

- [ ] **Step 3: Implement `scene.rs`**

Add to `crates/render/src/lib.rs`:
```rust
pub mod scene;
```

`crates/render/src/scene.rs`:
```rust
//! Turns a document plus view state into draw lists (spec §4.2 "Input", "Look", "Ghost").

use std::collections::HashMap;

use glam::Vec3;
use tsv_core::edit::ObjectId;
use tsv_core::ids::PortId;
use tsv_core::limits::Limits;
use tsv_core::model::{Document, Rgb};
use tsv_core::port_grid::Cell;

use crate::layout::{
    Aabb, FaceRect, device_box, device_name_rect, port_label_rect, port_marker_box, rack_name_rect,
    rack_parts, u_label_rect,
};

pub const BACKGROUND: Rgb = Rgb {
    r: 236,
    g: 239,
    b: 243,
};
pub const FRAME_COLOR: Rgb = Rgb {
    r: 70,
    g: 74,
    b: 82,
};
pub const GHOST_INVALID: Rgb = Rgb {
    r: 230,
    g: 40,
    b: 40,
};
pub const GHOST_ALPHA: f32 = 0.45;
/// Time for moved objects to (visually) reach their new position.
pub const MOTION_SECONDS: f32 = 0.12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxInstance {
    pub aabb: Aabb,
    /// Straight (non-premultiplied) RGBA, 0..1.
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub rect: FaceRect,
    pub color: [u8; 4],
}

/// The translucent preview of what is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ghost {
    pub aabb: Aabb,
    pub color: Rgb,
    /// `false` draws the ghost red: the drop would be rejected.
    pub valid: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SceneInput<'a> {
    /// The document to show: the committed one, or a previewed plan's document.
    pub document: &'a Document,
    pub limits: &'a Limits,
    pub selection: Option<ObjectId>,
    pub hovered_port: Option<PortId>,
    pub ghost: Option<Ghost>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub opaque: Vec<BoxInstance>,
    pub translucent: Vec<BoxInstance>,
    /// Boxes that get the selection outline.
    pub selected: Vec<BoxInstance>,
    pub labels: Vec<Label>,
}

pub fn rgba(c: Rgb, alpha: f32) -> [f32; 4] {
    [
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        alpha,
    ]
}

/// Black or white, whichever reads better on `background`.
pub fn contrast_text(background: Rgb) -> [u8; 4] {
    let luminance =
        0.2126 * background.r as f32 + 0.7152 * background.g as f32 + 0.0722 * background.b as f32;
    if luminance > 140.0 {
        [0, 0, 0, 255]
    } else {
        [255, 255, 255, 255]
    }
}

/// Resting positions (box minimum corners) of everything that animates when it moves.
pub fn animation_targets(doc: &Document, limits: &Limits) -> Vec<(ObjectId, Vec3)> {
    let mut targets = Vec::new();
    for (ri, rack) in doc.racks.iter().enumerate() {
        for device in &rack.devices {
            targets.push((ObjectId::Device(device.id), device_box(ri, device).min));
            for port in &device.ports {
                let cell = Cell {
                    row: port.row,
                    col: port.col,
                };
                targets.push((
                    ObjectId::Port(port.id),
                    port_marker_box(ri, device, cell, limits).min,
                ));
            }
        }
    }
    targets
}

/// Displayed positions that ease towards their targets (spec: ~120 ms).
#[derive(Debug, Clone, Default)]
pub struct Motion {
    displayed: HashMap<ObjectId, Vec3>,
}

impl Motion {
    /// Advances displayed positions by `dt_s` seconds. New objects appear at their target.
    /// Returns `true` while anything is still moving.
    pub fn update(&mut self, targets: &[(ObjectId, Vec3)], dt_s: f32) -> bool {
        let rate = 3.0 / MOTION_SECONDS;
        let blend = 1.0 - (-rate * dt_s.max(0.0)).exp();
        let mut moving = false;
        let mut next = HashMap::with_capacity(targets.len());
        for &(id, target) in targets {
            let shown = match self.displayed.get(&id) {
                Some(&p) => {
                    let p = p + (target - p) * blend;
                    if (target - p).length() < 0.05 {
                        target
                    } else {
                        p
                    }
                }
                None => target,
            };
            moving |= shown != target;
            next.insert(id, shown);
        }
        self.displayed = next;
        moving
    }

    /// Jumps every object to its target (e.g. after an import).
    pub fn snap(&mut self, targets: &[(ObjectId, Vec3)]) {
        self.displayed = targets.iter().copied().collect();
    }

    /// How far `id` is currently drawn from its resting position `target`.
    pub fn offset(&self, id: ObjectId, target: Vec3) -> Vec3 {
        self.displayed.get(&id).map_or(Vec3::ZERO, |p| *p - target)
    }
}

pub fn build_scene(input: &SceneInput, motion: &Motion) -> Scene {
    let mut scene = Scene::default();
    let doc = input.document;
    for (ri, rack) in doc.racks.iter().enumerate() {
        let rack_selected = input.selection == Some(ObjectId::Rack(rack.id));
        for part in rack_parts(ri, rack) {
            let b = BoxInstance {
                aabb: part,
                color: rgba(FRAME_COLOR, 1.0),
            };
            scene.opaque.push(b);
            if rack_selected {
                scene.selected.push(b);
            }
        }
        let frame_text = contrast_text(FRAME_COLOR);
        scene.labels.push(Label {
            text: rack.name.to_string(),
            rect: rack_name_rect(ri, rack),
            color: frame_text,
        });
        for u in 1..=rack.height_u {
            scene.labels.push(Label {
                text: u.to_string(),
                rect: u_label_rect(ri, u),
                color: frame_text,
            });
        }

        for device in &rack.devices {
            let id = ObjectId::Device(device.id);
            let resting = device_box(ri, device);
            let offset = motion.offset(id, resting.min);
            let b = BoxInstance {
                aabb: resting.translated(offset),
                color: rgba(device.color, 1.0),
            };
            scene.opaque.push(b);
            if input.selection == Some(id) {
                scene.selected.push(b);
            }
            scene.labels.push(Label {
                text: device.name.to_string(),
                rect: device_name_rect(ri, device).translated(offset),
                color: contrast_text(device.color),
            });

            let ink = contrast_text(device.color);
            let marker_color = Rgb {
                r: ink[0],
                g: ink[1],
                b: ink[2],
            };
            for port in &device.ports {
                let pid = ObjectId::Port(port.id);
                let cell = Cell {
                    row: port.row,
                    col: port.col,
                };
                let resting = port_marker_box(ri, device, cell, input.limits);
                let offset = motion.offset(pid, resting.min);
                let b = BoxInstance {
                    aabb: resting.translated(offset),
                    color: rgba(marker_color, 1.0),
                };
                scene.opaque.push(b);
                let selected = input.selection == Some(pid);
                if selected {
                    scene.selected.push(b);
                }
                if selected || input.hovered_port == Some(port.id) {
                    scene.labels.push(Label {
                        text: port.name.to_string(),
                        rect: port_label_rect(ri, device, cell, input.limits).translated(offset),
                        color: ink,
                    });
                }
            }
        }
    }
    if let Some(ghost) = input.ghost {
        let color = if ghost.valid {
            ghost.color
        } else {
            GHOST_INVALID
        };
        scene.translucent.push(BoxInstance {
            aabb: ghost.aabb,
            color: rgba(color, GHOST_ALPHA),
        });
    }
    scene
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test scene`
Expected: all 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/render
git commit -m "feat(render): build draw lists with labels, ghost and motion

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```


---

### Task 5: View state and port-placement camera mode (`view`)

**Files:**
- Create: `crates/render/src/view.rs`
- Modify: `crates/render/src/lib.rs`
- Test: `crates/render/tests/view.rs`

**Interfaces:**
- Consumes: `camera::{OrbitCamera, Tween}` (Task 3), `layout::FaceRect` (Task 1), `scene::{Motion, animation_targets}` (Task 4).
- Produces (`tsv_render::view::ViewState`):
  - `new(OrbitCamera)`, `camera() -> &OrbitCamera`, `camera_mut() -> &mut OrbitCamera` (cancels a running tween), `animate_to(OrbitCamera, now_s)`, `is_tweening()`.
  - `enter_port_mode(&FaceRect, aspect, now_s)`, `exit_port_mode(now_s)`, `in_port_mode()`.
  - `advance(&Document, &Limits, now_s: f64, dt_s: f32) -> bool` (true while anything animates).
  - `pub motion: Motion` (pass `&view.motion` to `build_scene`).

- [ ] **Step 1: Write the failing tests**

`crates/render/tests/view.rs`:
```rust
mod common;

use common::*;
use glam::{Vec2, Vec3};
use tsv_render::camera::{OrbitCamera, TWEEN_SECONDS};
use tsv_render::layout::FaceRect;
use tsv_render::view::ViewState;

fn start() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.5,
        pitch: 0.2,
        distance: 3000.0,
    }
}

fn face() -> FaceRect {
    FaceRect {
        min: Vec2::new(0.0, 0.0),
        max: Vec2::new(482.6, 44.45),
        z: 0.0,
    }
}

#[test]
fn camera_tween_runs_to_completion() {
    let d = doc(vec![]);
    let mut v = ViewState::new(start());
    let to = OrbitCamera {
        distance: 1000.0,
        ..start()
    };
    v.animate_to(to, 1.0);
    assert!(v.advance(&d, &limits(), 1.0 + TWEEN_SECONDS / 2.0, 0.016));
    assert!(!v.advance(&d, &limits(), 1.0 + TWEEN_SECONDS, 0.016));
    assert_eq!(*v.camera(), to);
}

#[test]
fn port_mode_returns_to_the_saved_camera() {
    let d = doc(vec![]);
    let mut v = ViewState::new(start());
    v.enter_port_mode(&face(), 1.5, 0.0);
    assert!(v.in_port_mode());
    v.advance(&d, &limits(), 1.0, 0.016);
    assert_eq!(*v.camera(), OrbitCamera::face_view(&face(), 1.5));
    v.exit_port_mode(2.0);
    assert!(!v.in_port_mode());
    v.advance(&d, &limits(), 3.0, 0.016);
    assert_eq!(*v.camera(), start());
}

#[test]
fn entering_port_mode_mid_tween_saves_the_tween_destination() {
    let d = doc(vec![]);
    let mut v = ViewState::new(start());
    let destination = OrbitCamera {
        distance: 1000.0,
        ..start()
    };
    v.animate_to(destination, 0.0);
    v.enter_port_mode(&face(), 1.5, 0.1);
    v.exit_port_mode(0.2);
    v.advance(&d, &limits(), 5.0, 0.016);
    assert_eq!(*v.camera(), destination);
}

#[test]
fn user_camera_control_cancels_a_tween() {
    let mut v = ViewState::new(start());
    v.animate_to(
        OrbitCamera {
            distance: 1000.0,
            ..start()
        },
        0.0,
    );
    v.camera_mut().zoom(0.0);
    assert!(!v.is_tweening());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p tsv-render --test view`
Expected: compile error, `unresolved import tsv_render::view`.

- [ ] **Step 3: Implement `view.rs`**

Add to `crates/render/src/lib.rs`:
```rust
pub mod view;
```

`crates/render/src/view.rs`:
```rust
//! Per-frame view state: camera, camera tweens, port-placement mode and object motion.

use tsv_core::limits::Limits;
use tsv_core::model::Document;

use crate::camera::{OrbitCamera, Tween};
use crate::layout::FaceRect;
use crate::scene::{Motion, animation_targets};

#[derive(Debug, Clone)]
pub struct ViewState {
    camera: OrbitCamera,
    tween: Option<Tween>,
    saved: Option<OrbitCamera>,
    pub motion: Motion,
}

impl ViewState {
    pub fn new(camera: OrbitCamera) -> ViewState {
        ViewState {
            camera,
            tween: None,
            saved: None,
            motion: Motion::default(),
        }
    }

    pub fn camera(&self) -> &OrbitCamera {
        &self.camera
    }

    /// The camera for direct user control (orbit, pan, zoom); cancels any running tween.
    pub fn camera_mut(&mut self) -> &mut OrbitCamera {
        self.tween = None;
        &mut self.camera
    }

    /// Starts an eased move from the current camera to `to`.
    pub fn animate_to(&mut self, to: OrbitCamera, now_s: f64) {
        self.tween = Some(Tween {
            from: self.camera,
            to,
            start_s: now_s,
        });
    }

    pub fn is_tweening(&self) -> bool {
        self.tween.is_some()
    }

    /// Saves the current camera (or the current tween's destination) and moves to `face`.
    pub fn enter_port_mode(&mut self, face: &FaceRect, aspect: f32, now_s: f64) {
        if self.saved.is_none() {
            self.saved = Some(self.tween.map_or(self.camera, |t| t.to));
        }
        self.animate_to(OrbitCamera::face_view(face, aspect), now_s);
    }

    /// Moves back to the camera saved by `enter_port_mode`. Does nothing outside port mode.
    pub fn exit_port_mode(&mut self, now_s: f64) {
        if let Some(saved) = self.saved.take() {
            self.animate_to(saved, now_s);
        }
    }

    pub fn in_port_mode(&self) -> bool {
        self.saved.is_some()
    }

    /// Advances the camera tween to `now_s` and object motion by `dt_s`.
    /// Returns `true` while anything is still animating (keep requesting frames).
    pub fn advance(&mut self, doc: &Document, limits: &Limits, now_s: f64, dt_s: f32) -> bool {
        if let Some(tween) = self.tween {
            let (camera, done) = tween.sample(now_s);
            self.camera = camera;
            if done {
                self.tween = None;
            }
        }
        let moving = self.motion.update(&animation_targets(doc, limits), dt_s);
        self.tween.is_some() || moving
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test view`
Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/render
git commit -m "feat(render): add view state with camera tweens and port mode

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```


---

### Task 6: Text rasterising and the wgpu renderer (`text`, `gpu`)

**Files:**
- Create: `crates/render/src/text.rs`, `crates/render/src/gpu/mod.rs`, `crates/render/src/gpu/shaders.wgsl`
- Modify: `crates/render/src/lib.rs`
- Test: `crates/render/tests/text.rs`, `crates/render/tests/gpu.rs`

**Interfaces:**
- Consumes: `camera::OrbitCamera` (Task 3), `scene::{Scene, BoxInstance, Label, BACKGROUND}` (Task 4).
- Produces:
  - `tsv_render::text::TextRasterizer` trait: `rasterize(&mut self, text, width, height, color: [u8; 4]) -> Vec<u8>` (RGBA8, straight alpha, top row first).
  - `tsv_render::text::{LABEL_PX_PER_MM, MAX_LABEL_PX, label_texture_size(width_mm, height_mm) -> (u32, u32)}`.
  - `tsv_render::text::CanvasTextRasterizer` (wasm32 only): `new() -> Result<Self, JsValue>`, browser 2D-canvas text.
  - `tsv_render::gpu::RenderError { NoAdapter(String), NoDevice(String), Surface(String) }` (`Display`, `Error`).
  - `tsv_render::gpu::Renderer`: `new_offscreen(w, h).await`, `for_canvas(HtmlCanvasElement).await` (wasm32 only), `size()`, `resize(w, h)` (clamps to ≥ 1), `render(&Scene, &OrbitCamera, &mut dyn TextRasterizer) -> Result<(), RenderError>`, `is_device_lost()`, `read_pixels() -> Option<Vec<u8>>` (native only, offscreen only).
  - `tsv_render::gpu::webgpu_available().await -> bool` (wasm32 only).

- [ ] **Step 1: Write the failing tests**

`crates/render/tests/text.rs`:
```rust
use tsv_render::text::{MAX_LABEL_PX, label_texture_size};

#[test]
fn label_textures_have_four_pixels_per_millimetre() {
    assert_eq!(label_texture_size(10.0, 2.5), (40, 10));
}

#[test]
fn label_texture_size_is_clamped() {
    assert_eq!(label_texture_size(0.0, 10_000.0), (1, MAX_LABEL_PX));
}
```

`crates/render/tests/gpu.rs`:
```rust
//! Pixel tests on a real (possibly software) GPU adapter. Skipped when none is available.

use glam::{Vec2, Vec3};
use tsv_render::camera::OrbitCamera;
use tsv_render::gpu::Renderer;
use tsv_render::layout::{Aabb, FaceRect};
use tsv_render::scene::{BACKGROUND, BoxInstance, Label, Scene};
use tsv_render::text::TextRasterizer;

const SIZE: u32 = 64;

/// Fills the whole label texture with the label colour.
struct SolidText;

impl TextRasterizer for SolidText {
    fn rasterize(&mut self, _text: &str, width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        color.repeat((width * height) as usize)
    }
}

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::new_offscreen(SIZE, SIZE)) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

/// Looks straight at the origin from 1 m; a ±100 mm box spans pixels 24..40.
fn camera() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        distance: 1000.0,
    }
}

fn red_box() -> BoxInstance {
    BoxInstance {
        aabb: Aabb::new(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 0.0),
        ),
        color: [1.0, 0.0, 0.0, 1.0],
    }
}

fn draw(r: &mut Renderer, scene: &Scene) -> Vec<u8> {
    r.render(scene, &camera(), &mut SolidText).unwrap();
    r.read_pixels().unwrap()
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn background() -> [u8; 4] {
    [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b, 255]
}

#[test]
fn draws_a_shaded_box_on_the_background() {
    let Some(mut r) = renderer() else { return };
    let scene = Scene {
        opaque: vec![red_box()],
        ..Default::default()
    };
    let p = draw(&mut r, &scene);
    let [red, green, blue, _] = pixel(&p, 32, 32);
    assert!(
        (195..=212).contains(&red) && green < 8 && blue < 8,
        "front face {red},{green},{blue}"
    );
    assert_eq!(pixel(&p, 2, 2), background());
}

#[test]
fn selection_draws_a_glow_just_outside_the_box() {
    let Some(mut r) = renderer() else { return };
    let plain = Scene {
        opaque: vec![red_box()],
        ..Default::default()
    };
    assert_eq!(pixel(&draw(&mut r, &plain), 22, 32), background());
    let selected = Scene {
        selected: vec![red_box()],
        ..plain
    };
    let p = draw(&mut r, &selected);
    let [red, green, blue, _] = pixel(&p, 22, 32);
    assert!(
        red > 240 && (150..220).contains(&green) && blue < 120,
        "glow {red},{green},{blue}"
    );
    assert_eq!(
        pixel(&p, 10, 32),
        background(),
        "glow fades within a few pixels"
    );
}

#[test]
fn labels_show_their_texture() {
    let Some(mut r) = renderer() else { return };
    let label = Label {
        text: "DMM".into(),
        rect: FaceRect {
            min: Vec2::splat(-20.0),
            max: Vec2::splat(20.0),
            z: 1.0,
        },
        color: [0, 0, 255, 255],
    };
    let scene = Scene {
        opaque: vec![red_box()],
        labels: vec![label],
        ..Default::default()
    };
    assert_eq!(pixel(&draw(&mut r, &scene), 32, 32), [0, 0, 255, 255]);
}

#[test]
fn ghost_is_blended_over_the_scene() {
    let Some(mut r) = renderer() else { return };
    let ghost = BoxInstance {
        aabb: Aabb::new(
            Vec3::new(-300.0, -20.0, 50.0),
            Vec3::new(-150.0, 20.0, 60.0),
        ),
        color: [0.9, 0.15, 0.15, 0.45],
    };
    let scene = Scene {
        translucent: vec![ghost],
        ..Default::default()
    };
    let [red, green, blue, _] = pixel(&draw(&mut r, &scene), 15, 32);
    assert!(
        red > green + 40 && green > 100 && blue > 100,
        "reddish tint {red},{green},{blue}"
    );
}

#[test]
fn resize_changes_the_output_size() {
    let Some(mut r) = renderer() else { return };
    r.resize(32, 16);
    assert_eq!(r.size(), (32, 16));
    r.render(&Scene::default(), &camera(), &mut SolidText)
        .unwrap();
    assert_eq!(r.read_pixels().unwrap().len(), 32 * 16 * 4);
}

#[test]
fn zero_size_is_clamped_to_one_pixel() {
    let Some(mut r) = renderer() else { return };
    r.resize(0, 0);
    assert_eq!(r.size(), (1, 1));
    r.render(&Scene::default(), &camera(), &mut SolidText)
        .unwrap();
    assert_eq!(r.read_pixels().unwrap().len(), 4);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p tsv-render --test text --test gpu`
Expected: compile errors, `unresolved import tsv_render::text` and `unresolved import tsv_render::gpu`.

- [ ] **Step 3: Implement `text.rs`**

Add to `crates/render/src/lib.rs`:
```rust
pub mod text;
```

`crates/render/src/text.rs`:
```rust
//! Label text rasterisation (spec §4.2 "Text").

/// Pixels per millimetre of label texture resolution.
pub const LABEL_PX_PER_MM: f32 = 4.0;
pub const MAX_LABEL_PX: u32 = 2048;

/// Draws `text` centred in a `width` × `height` RGBA8 image (straight alpha, row-major,
/// top row first), scaled to fit, in `color`. Everything else is transparent.
pub trait TextRasterizer {
    fn rasterize(&mut self, text: &str, width: u32, height: u32, color: [u8; 4]) -> Vec<u8>;
}

/// Texture size for a label rectangle of `width_mm` × `height_mm`.
pub fn label_texture_size(width_mm: f32, height_mm: f32) -> (u32, u32) {
    let px = |mm: f32| ((mm * LABEL_PX_PER_MM).round() as u32).clamp(1, MAX_LABEL_PX);
    (px(width_mm), px(height_mm))
}

/// Rasteriser using the browser's 2D canvas text engine.
#[cfg(target_arch = "wasm32")]
pub struct CanvasTextRasterizer {
    canvas: web_sys::OffscreenCanvas,
    context: web_sys::OffscreenCanvasRenderingContext2d,
}

#[cfg(target_arch = "wasm32")]
impl CanvasTextRasterizer {
    pub fn new() -> Result<CanvasTextRasterizer, wasm_bindgen::JsValue> {
        use wasm_bindgen::JsCast;
        let canvas = web_sys::OffscreenCanvas::new(1, 1)?;
        let context = canvas
            .get_context("2d")?
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("no 2d context"))?
            .dyn_into::<web_sys::OffscreenCanvasRenderingContext2d>()?;
        Ok(CanvasTextRasterizer { canvas, context })
    }
}

#[cfg(target_arch = "wasm32")]
impl TextRasterizer for CanvasTextRasterizer {
    fn rasterize(&mut self, text: &str, width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        let ctx = &self.context;
        ctx.clear_rect(0.0, 0.0, width as f64, height as f64);
        let mut font_px = height as f64 * 0.8;
        ctx.set_font(&format!("600 {font_px}px system-ui, sans-serif"));
        if let Ok(metrics) = ctx.measure_text(text) {
            let max_width = width as f64 * 0.95;
            if metrics.width() > max_width {
                font_px *= max_width / metrics.width();
                ctx.set_font(&format!("600 {font_px}px system-ui, sans-serif"));
            }
        }
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        ctx.set_fill_style_str(&format!(
            "rgba({}, {}, {}, {})",
            color[0],
            color[1],
            color[2],
            color[3] as f64 / 255.0
        ));
        let _ = ctx.fill_text(text, width as f64 / 2.0, height as f64 / 2.0);
        ctx.get_image_data(0.0, 0.0, width as f64, height as f64)
            .map(|data| data.data().0)
            .unwrap_or_else(|_| vec![0; (width * height * 4) as usize])
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test text`
Expected: both tests PASS (`gpu` still fails to compile; that is next).

- [ ] **Step 5: Implement the shaders and the renderer**

Add to `crates/render/src/lib.rs`:
```rust
pub mod gpu;
```

`crates/render/src/gpu/shaders.wgsl`:
```wgsl
// Boxes (opaque, translucent ghost, selection mask), labels and the selection outline.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the light (unit length).
    light_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct BoxIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) box_min: vec3<f32>,
    @location(3) box_max: vec3<f32>,
    @location(4) color: vec4<f32>,
};

struct BoxOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_box(v: BoxIn) -> BoxOut {
    let world = v.box_min + v.position * (v.box_max - v.box_min);
    var out: BoxOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.normal = v.normal;
    out.color = v.color;
    return out;
}

@fragment
fn fs_box(in: BoxOut) -> @location(0) vec4<f32> {
    let light = 0.55 + 0.45 * max(dot(normalize(in.normal), globals.light_dir.xyz), 0.0);
    return vec4<f32>(in.color.rgb * light, in.color.a);
}

@fragment
fn fs_mask(in: BoxOut) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

@group(1) @binding(0) var label_texture: texture_2d<f32>;
@group(1) @binding(1) var label_sampler: sampler;

struct LabelIn {
    // min.x, min.y, max.x, max.y of the label rectangle (world units).
    @location(0) rect: vec4<f32>,
    @location(1) z: f32,
};

struct LabelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Four vertices as a triangle strip; uv (0, 0) is the top-left of the texture.
@vertex
fn vs_label(@builtin(vertex_index) index: u32, l: LabelIn) -> LabelOut {
    let uv = vec2<f32>(f32(index & 1u), f32(index >> 1u));
    let x = mix(l.rect.x, l.rect.z, uv.x);
    let y = mix(l.rect.w, l.rect.y, uv.y);
    var out: LabelOut;
    out.clip = globals.view_proj * vec4<f32>(x, y, l.z, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_label(in: LabelOut) -> @location(0) vec4<f32> {
    return textureSample(label_texture, label_sampler, in.uv);
}

@group(0) @binding(0) var selection_mask: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

const GLOW_RADIUS: i32 = 3;

// Glow on pixels just outside the selection mask, fading with distance.
@fragment
fn fs_outline(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let centre = vec2<i32>(position.xy);
    let last = vec2<i32>(textureDimensions(selection_mask)) - vec2<i32>(1, 1);
    if (textureLoad(selection_mask, centre, 0).r > 0.5) {
        discard;
    }
    var strength = 0.0;
    for (var dy = -GLOW_RADIUS; dy <= GLOW_RADIUS; dy++) {
        for (var dx = -GLOW_RADIUS; dx <= GLOW_RADIUS; dx++) {
            let q = clamp(centre + vec2<i32>(dx, dy), vec2<i32>(0, 0), last);
            if (textureLoad(selection_mask, q, 0).r > 0.5) {
                let d = length(vec2<f32>(f32(dx), f32(dy)));
                strength = max(strength, 1.0 - (d - 1.0) / f32(GLOW_RADIUS));
            }
        }
    }
    if (strength <= 0.0) {
        discard;
    }
    return vec4<f32>(1.0, 0.65, 0.0, clamp(strength, 0.0, 1.0));
}
```

`crates/render/src/gpu/mod.rs`:
```rust
//! wgpu renderer (spec §4.2): instanced boxes, labels, translucent ghost, selection outline.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::camera::OrbitCamera;
use crate::scene::{BACKGROUND, BoxInstance, Label, Scene};
use crate::text::{TextRasterizer, label_texture_size};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The browser or system offers no usable WebGPU adapter.
    NoAdapter(String),
    NoDevice(String),
    Surface(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::NoAdapter(e) => write!(f, "no WebGPU adapter: {e}"),
            RenderError::NoDevice(e) => write!(f, "could not create a GPU device: {e}"),
            RenderError::Surface(e) => write!(f, "could not use the canvas: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CubeVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BoxRaw {
    min: [f32; 3],
    max: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LabelRaw {
    rect: [f32; 4],
    z: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LabelKey {
    text: String,
    color: [u8; 4],
    width: u32,
    height: u32,
}

enum Target {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    Offscreen {
        texture: wgpu::Texture,
    },
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: Target,
    width: u32,
    height: u32,
    depth: wgpu::TextureView,
    mask: wgpu::TextureView,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    cube: wgpu::Buffer,
    box_pipeline: wgpu::RenderPipeline,
    ghost_pipeline: wgpu::RenderPipeline,
    mask_pipeline: wgpu::RenderPipeline,
    label_pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    label_layout: wgpu::BindGroupLayout,
    outline_layout: wgpu::BindGroupLayout,
    outline_bind: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    labels: HashMap<LabelKey, wgpu::BindGroup>,
    lost: Arc<AtomicBool>,
}

fn instance() -> wgpu::Instance {
    let backends = if cfg!(target_arch = "wasm32") {
        wgpu::Backends::BROWSER_WEBGPU
    } else {
        wgpu::Backends::PRIMARY
    };
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends;
    wgpu::Instance::new(desc)
}

async fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), RenderError> {
    adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .map_err(|e| RenderError::NoDevice(e.to_string()))
}

/// Whether this browser exposes a working WebGPU implementation.
#[cfg(target_arch = "wasm32")]
pub async fn webgpu_available() -> bool {
    wgpu::util::is_browser_webgpu_supported().await
}

impl Renderer {
    /// Renders into an offscreen RGBA8 texture (tests; `read_pixels` reads it back).
    pub async fn new_offscreen(width: u32, height: u32) -> Result<Renderer, RenderError> {
        let instance = instance();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = request_device(&adapter).await?;
        let texture = create_offscreen_texture(&device, width, height);
        Ok(Self::from_parts(
            device,
            queue,
            Target::Offscreen { texture },
            OFFSCREEN_FORMAT,
            width,
            height,
        ))
    }

    /// Renders into a page canvas through WebGPU.
    #[cfg(target_arch = "wasm32")]
    pub async fn for_canvas(canvas: web_sys::HtmlCanvasElement) -> Result<Renderer, RenderError> {
        let (width, height) = (canvas.width().max(1), canvas.height().max(1));
        let instance = instance();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| RenderError::Surface(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = request_device(&adapter).await?;
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| RenderError::Surface("canvas not supported by adapter".into()))?;
        let caps = surface.get_capabilities(&adapter);
        config.format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(config.format);
        surface.configure(&device, &config);
        let format = config.format;
        Ok(Self::from_parts(
            device,
            queue,
            Target::Surface { surface, config },
            format,
            width,
            height,
        ))
    }

    fn from_parts(
        device: wgpu::Device,
        queue: wgpu::Queue,
        target: Target,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Renderer {
        let lost = Arc::new(AtomicBool::new(false));
        let flag = lost.clone();
        device.set_device_lost_callback(move |_, _| flag.store(true, Ordering::SeqCst));

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tsv shaders"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let label_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("label"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let outline_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("outline"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("label"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let box_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("box"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let label_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("label"),
                bind_group_layouts: &[Some(&globals_layout), Some(&label_layout)],
                immediate_size: 0,
            });
        let outline_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("outline"),
                bind_group_layouts: &[Some(&outline_layout)],
                immediate_size: 0,
            });

        let box_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<CubeVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<BoxRaw>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4],
            }),
        ];
        let label_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LabelRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32],
        })];

        let depth_state = |write: bool| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let culled = wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        };
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        primitive: wgpu::PrimitiveState,
                        depth: Option<wgpu::DepthStencilState>,
                        target: wgpu::ColorTargetState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive,
                depth_stencil: depth,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(target)],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let opaque_target = wgpu::ColorTargetState::from(format);
        let blended_target = wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let box_pipeline = pipeline(
            "box",
            &box_layout,
            "vs_box",
            "fs_box",
            &box_buffers,
            culled,
            Some(depth_state(true)),
            opaque_target,
        );
        let ghost_pipeline = pipeline(
            "ghost",
            &box_layout,
            "vs_box",
            "fs_box",
            &box_buffers,
            culled,
            Some(depth_state(false)),
            blended_target.clone(),
        );
        let mask_pipeline = pipeline(
            "mask",
            &box_layout,
            "vs_box",
            "fs_mask",
            &box_buffers,
            culled,
            None,
            wgpu::ColorTargetState::from(MASK_FORMAT),
        );
        let strip = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        };
        let label_pipeline = pipeline(
            "label",
            &label_pipeline_layout,
            "vs_label",
            "fs_label",
            &label_buffers,
            strip,
            Some(depth_state(false)),
            blended_target.clone(),
        );
        let outline_pipeline = pipeline(
            "outline",
            &outline_pipeline_layout,
            "vs_fullscreen",
            "fs_outline",
            &[],
            wgpu::PrimitiveState::default(),
            None,
            blended_target,
        );

        let cube_vertices = unit_cube();
        let cube = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube"),
            size: std::mem::size_of_val(cube_vertices.as_slice()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&cube, 0, bytemuck::cast_slice(&cube_vertices));

        let (depth, mask) = create_size_dependent(&device, width, height);
        let outline_bind = create_outline_bind(&device, &outline_layout, &mask);
        Renderer {
            device,
            queue,
            target,
            width,
            height,
            depth,
            mask,
            globals,
            globals_bind,
            cube,
            box_pipeline,
            ghost_pipeline,
            mask_pipeline,
            label_pipeline,
            outline_pipeline,
            label_layout,
            outline_layout,
            outline_bind,
            sampler,
            labels: HashMap::new(),
            lost,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// `true` once the GPU device has been lost; the app should offer a reload.
    pub fn is_device_lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    /// Resizes the drawing surface (canvas pixel size).
    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) == (self.width, self.height) {
            return;
        }
        self.width = width;
        self.height = height;
        match &mut self.target {
            Target::Surface { surface, config } => {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
            Target::Offscreen { texture } => {
                *texture = create_offscreen_texture(&self.device, width, height);
            }
        }
        let (depth, mask) = create_size_dependent(&self.device, width, height);
        self.depth = depth;
        self.mask = mask;
        self.outline_bind = create_outline_bind(&self.device, &self.outline_layout, &self.mask);
    }

    /// Draws one frame. Skips the frame (Ok) when the surface is temporarily unavailable.
    pub fn render(
        &mut self,
        scene: &Scene,
        camera: &OrbitCamera,
        text: &mut dyn TextRasterizer,
    ) -> Result<(), RenderError> {
        let surface_texture = match &self.target {
            Target::Surface { surface, config } => match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    surface.configure(&self.device, config);
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(RenderError::Surface(
                        "validation error acquiring frame".into(),
                    ));
                }
                _ => return Ok(()),
            },
            Target::Offscreen { .. } => None,
        };
        let view = match (&surface_texture, &self.target) {
            (Some(t), _) => t
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            (None, Target::Offscreen { texture }) => {
                texture.create_view(&wgpu::TextureViewDescriptor::default())
            }
            (None, Target::Surface { .. }) => unreachable!("surface frames always have a texture"),
        };

        let aspect = self.width as f32 / self.height as f32;
        let light = Vec3::new(0.4, 0.8, 0.6).normalize();
        let globals = Globals {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            light_dir: [light.x, light.y, light.z, 0.0],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let label_binds = self.prepare_labels(&scene.labels, text);
        let opaque = self.instance_buffer("opaque", &scene.opaque);
        let translucent = self.instance_buffer("translucent", &scene.translucent);
        let selected = self.instance_buffer("selected", &scene.selected);
        let label_raw: Vec<LabelRaw> = scene
            .labels
            .iter()
            .map(|l| LabelRaw {
                rect: [l.rect.min.x, l.rect.min.y, l.rect.max.x, l.rect.max.y],
                z: l.rect.z,
            })
            .collect();
        let label_buffer = self.vertex_buffer("labels", bytemuck::cast_slice(&label_raw));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let bg = BACKGROUND;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg.r as f64 / 255.0,
                            g: bg.g as f64 / 255.0,
                            b: bg.b as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            if let Some(buffer) = &opaque {
                pass.set_pipeline(&self.box_pipeline);
                pass.set_vertex_buffer(0, self.cube.slice(..));
                pass.set_vertex_buffer(1, buffer.slice(..));
                pass.draw(0..36, 0..scene.opaque.len() as u32);
            }
            if let Some(buffer) = &label_buffer {
                pass.set_pipeline(&self.label_pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for (i, bind) in label_binds.iter().enumerate() {
                    pass.set_bind_group(1, bind, &[]);
                    pass.draw(0..4, i as u32..i as u32 + 1);
                }
            }
            if let Some(buffer) = &translucent {
                pass.set_pipeline(&self.ghost_pipeline);
                pass.set_vertex_buffer(0, self.cube.slice(..));
                pass.set_vertex_buffer(1, buffer.slice(..));
                pass.draw(0..36, 0..scene.translucent.len() as u32);
            }
        }
        if let Some(buffer) = &selected {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("selection mask"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.mask,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.mask_pipeline);
                pass.set_bind_group(0, &self.globals_bind, &[]);
                pass.set_vertex_buffer(0, self.cube.slice(..));
                pass.set_vertex_buffer(1, buffer.slice(..));
                pass.draw(0..36, 0..scene.selected.len() as u32);
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("outline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.outline_pipeline);
            pass.set_bind_group(0, &self.outline_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        if let Some(t) = surface_texture {
            self.queue.present(t);
        }
        Ok(())
    }

    /// Reads the offscreen target back as tightly packed RGBA8 rows (top row first).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_pixels(&self) -> Option<Vec<u8>> {
        let Target::Offscreen { texture } = &self.target else {
            return None;
        };
        let unpadded = self.width * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * self.height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = buffer.slice(..).get_mapped_range().ok()?;
        let mut pixels = Vec::with_capacity((unpadded * self.height) as usize);
        for row in data.chunks(padded as usize) {
            pixels.extend_from_slice(&row[..unpadded as usize]);
        }
        Some(pixels)
    }

    fn vertex_buffer(&self, label: &str, bytes: &[u8]) -> Option<wgpu::Buffer> {
        if bytes.is_empty() {
            return None;
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, bytes);
        Some(buffer)
    }

    fn instance_buffer(&self, label: &str, boxes: &[BoxInstance]) -> Option<wgpu::Buffer> {
        let raw: Vec<BoxRaw> = boxes
            .iter()
            .map(|b| BoxRaw {
                min: b.aabb.min.to_array(),
                max: b.aabb.max.to_array(),
                color: b.color,
            })
            .collect();
        self.vertex_buffer(label, bytemuck::cast_slice(&raw))
    }

    /// Bind groups for every label, in order; rasterises labels not seen last frame and drops
    /// cached textures that are no longer used.
    fn prepare_labels(
        &mut self,
        labels: &[Label],
        text: &mut dyn TextRasterizer,
    ) -> Vec<wgpu::BindGroup> {
        let mut used = HashMap::with_capacity(labels.len());
        let mut binds = Vec::with_capacity(labels.len());
        for label in labels {
            let size = label.rect.size();
            let (width, height) = label_texture_size(size.x, size.y);
            let key = LabelKey {
                text: label.text.clone(),
                color: label.color,
                width,
                height,
            };
            let bind = match self.labels.remove(&key).or_else(|| used.get(&key).cloned()) {
                Some(bind) => bind,
                None => {
                    let pixels = text.rasterize(&label.text, width, height, label.color);
                    self.create_label_bind(&pixels, width, height)
                }
            };
            binds.push(bind.clone());
            used.insert(key, bind);
        }
        self.labels = used;
        binds
    }

    fn create_label_bind(&self, pixels: &[u8], width: u32, height: u32) -> wgpu::BindGroup {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("label"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("label"),
            layout: &self.label_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

fn create_offscreen_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn create_size_dependent(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let make = |label: &str, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    (
        make(
            "depth",
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
        make(
            "selection mask",
            MASK_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        ),
    )
}

fn create_outline_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    mask: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("outline"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(mask),
        }],
    })
}

/// 36 vertices of the unit cube [0, 1]³, counter-clockwise when seen from outside.
fn unit_cube() -> Vec<CubeVertex> {
    // Each face: outward normal and its four corners counter-clockwise seen from outside.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [0.0, 0.0, 1.0],
            [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
        ),
        (
            [0.0, 0.0, -1.0],
            [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]],
        ),
        (
            [1.0, 0.0, 0.0],
            [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]],
        ),
        (
            [-1.0, 0.0, 0.0],
            [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]],
        ),
        (
            [0.0, 1.0, 0.0],
            [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]],
        ),
        (
            [0.0, -1.0, 0.0],
            [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
        ),
    ];
    faces
        .iter()
        .flat_map(|(normal, c)| {
            [c[0], c[1], c[2], c[0], c[2], c[3]].map(|position| CubeVertex {
                position,
                normal: *normal,
            })
        })
        .collect()
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p tsv-render --test gpu -- --nocapture`
Expected: all 6 tests PASS and no `skipping GPU test` line is printed. If it is printed, the machine has no usable adapter: install Mesa's Vulkan drivers (`mesa-vulkan-drivers` on Ubuntu, which provides the `llvmpipe` software adapter) and re-run; a skipped run does not complete this task.

- [ ] **Step 7: Whole workspace, both targets, formatter and linter**

Run: `cargo test --workspace`
Expected: everything passes: `core` 115; `render`: layout 9, pick 9, camera 11, scene 7, view 4, text 2, gpu 6.

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p tsv-render --target wasm32-unknown-unknown -- -D warnings`
Expected: no warnings on either target. If `cargo fmt` changed files, re-run `cargo test --workspace` before committing.

- [ ] **Step 8: Commit**

```bash
git add crates/render
git commit -m "feat(render): add wgpu renderer with labels, ghost and selection outline

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

