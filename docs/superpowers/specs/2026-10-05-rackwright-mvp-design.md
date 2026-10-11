# TestStationView — MVP Design

Date: 2026-10-05
Status: Draft for review

## 1. Purpose

TestStationView is a browser application for the test-engineering team to plan and document
rack-mounted test stations in 3D. Users build a station from racks, place measurement equipment in
the racks, and add named ports/connectors to the equipment.

The MVP is a planning/documentation tool. The long-term direction is a digital twin of real,
existing stations (possibly with live data and cabling). The MVP does not implement any of that,
but its data model and storage layer must not block it.

### Users and environment

- Users: the test-engineering team.
- Platform: mostly Windows with Chrome or Firefox.
- Delivery: Rust compiled to WebAssembly, served as static files from a public HTTPS site.
- Graphics: WebGPU only. No WebGL fallback in the MVP.

### Scale

At most about 5 racks per document, 10 devices per rack and 10 ports per device.

## 2. Scope

### In scope

- A 3D view of one or more racks standing side by side in a row, with all equipment and ports.
- Adding, moving, resizing, renaming and removing racks, equipment and ports, live.
- Names shown in the 3D view.
- A tree list with two-way, single selection between the tree and the 3D view, highlighted in 3D.
- Undo/redo of all document changes.
- Autosave to browser local storage, plus JSON export/import.

### Out of scope

- Backend or server persistence. Storage sits behind an interface so one can be added later.
- Equipment catalog / types. Equipment is ad hoc, and the model reserves a `kind` field.
- Port connector types and cables. Ports reserve a `kind` field and have stable IDs.
- Half-width, fractional-height, configurable-depth or rear-mounted equipment.
- Rear-face ports.
- Free placement or rotation of racks; gaps between racks.
- Multi-select.
- Multiple documents in local storage.
- Cross-tab synchronisation.
- WebGL2 fallback.
- Automated visual testing of 3D output.
- Choice of hosting provider, CI pipeline, deployment automation.

## 3. Domain rules

### 3.1 Document

- A document has a **document name** and an ordered list of racks. List order is the order of the
  racks in the row, left to right.
- A new document is named `Station1` and contains one 42U rack named `Rack1`.
- Document name rules (Windows-filename-safe):
  - Leading/trailing whitespace is trimmed. Inner spaces are allowed.
  - Must not be empty after trimming.
  - Must not contain `< > : " / \ | ? *` or control characters.
  - Must not end with `.` or a space.
  - Must not be a reserved Windows name: `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
    `LPT1`–`LPT9` (case-insensitive).
  - At most 100 characters.
  - Invalid input is rejected with a message, never silently corrected.

### 3.2 Names of racks, devices and ports

- **Normalisation:** every Unicode whitespace character is removed (`"DMM 1"` → `"DMM1"`).
- **Validation:** must not be empty after normalisation, and must have at most `name_max_len`
  characters (Unicode scalar values). `name_max_len` is 10 in the MVP and lives in the configurable
  `Limits`.
- **Uniqueness:** compared case-insensitively (lowercase forms):
  - Racks: unique within the document.
  - Devices: unique within their rack.
  - Ports: unique within their device.
- **Auto-rename:** applied when a device enters a rack (new device drop, or move from another rack)
  and its name clashes. Try `base_2`, `base_3`, … and take the first free one. If `base_N` would
  exceed `name_max_len`, truncate `base` so that it fits (e.g. `MultiMeter` → `MultiMet_2`; with
  `_10`, `MultiMe_10`).
- Racks and ports never auto-rename: rack names come from user input or `RackN` generation, and the
  new-port form checks uniqueness before the drag can start.

### 3.3 Racks

- Width is always 19". Height is configurable in whole rack units (U).
- U positions are numbered from the bottom: U1 is the lowest slot.
- **Add rack:** an "Add rack" button below the tree appends a rack at the end of the row. Its height
  is the height of the last rack in the row (42U if there are no racks). Its name is the first free
  `RackN` (`Rack1`, `Rack2`, …).
- **Height limit:** at most `max_rack_height_u` (100U in the MVP, in `Limits`); higher values are
  rejected by edits and by import validation.
- **Height change:** rejected if any device's top U would exceed the new height. Racks never grow
  automatically.
- **Removal:** removing a rack removes all its devices and ports. If the rack contains any devices,
  the user must confirm first.
- **Reorder:** by dragging rack nodes in the tree.

### 3.4 Devices (equipment)

- Full-width (19"), front-mounted, whole-U height (≥ 1), with a colour (any RGB). New devices are
  neutral grey.
- A device occupies `[bottom_u, bottom_u + height_u − 1]` and must lie within `[1, rack.height_u]`.
- Devices in a rack never overlap.

#### Placement and push (drop of a new or moved device)

Given a dropped device `D` with height `h` at snapped bottom position `u`:

1. Clamp `u` to `[1, rack.height_u − h + 1]`. If `h > rack.height_u`, the drop is rejected.
2. Let `c = u + (h − 1) / 2` (`D`'s centre, as a real number).
3. Take every other device in the target rack (excluding `D` itself when it moves within the same
   rack). Its centre is `b + (height − 1) / 2`.
   - **Up group:** centre `> c`.
   - **Down group:** centre `≤ c`. A device whose centre equals `c` therefore goes down.
4. **Up pass:** sort the up group by ascending bottom. `cursor = u + h`. For each device: if its
   bottom `< cursor`, set its bottom to `cursor`. Then `cursor = bottom + height`. If any device's top
   exceeds `rack.height_u`, the drop is rejected.
5. **Down pass:** sort the down group by descending top. `cursor = u − 1` (the highest free U below
   `D`). For each device: if its top `> cursor`, move it so its top equals `cursor`. Then
   `cursor = bottom − 1`. If any device's bottom falls below 1, the drop is rejected.
6. Devices that don't collide don't move.
7. If the device came from another rack (or is new) and its name clashes in the target rack, it is
   auto-renamed (§3.2).
8. **Rejected** means the document doesn't change at all. There is no partial result, and the
   rack never expands.

#### Height change

- **Grow:** the bottom stays fixed and the device extends in the +U direction. Then only the up
  pass is run (with `D` at its new size). Rejected if that pass fails.
- **Shrink:** rejected if any of the device's ports would have `row ≥` the new row count.
  Otherwise applied (it can't cause collisions).

#### Removal

Removing a device removes its ports.

### 3.5 Ports

- Front face only. A port belongs to exactly one device and can never move to another device.
- The front face is a grid with `height_u × port_rows_per_u` rows and `port_cols` columns. In the
  MVP, `port_rows_per_u = 1` and `port_cols = 5`, both configurable in `Limits`.
- A port occupies exactly one cell `(row, col)`. Ports never share a cell.
- Ports are listed in reading order: rows top to bottom, then columns left to right.

#### Placement and push (drop of a new or moved port)

1. When an existing port moves, its old cell is vacated first.
2. If the target cell is free, the port goes there.
3. If it is occupied, the push direction comes from the cursor's offset `(dx, dy)` from the centre
   of the target cell, in face coordinates:
   - If `|dx| ≥ |dy|` (horizontal wins ties, including the exact centre): cursor left of centre or
     exactly at centre pushes **right**; cursor right of centre pushes **left**.
   - Otherwise: cursor below centre pushes **up**; cursor above centre pushes **down**.
4. The occupant moves one cell in that direction. If that cell is occupied, that port moves one more
   cell in the same direction, and so on (cascade).
5. If the chain would push any port off the grid, the drop is rejected and nothing changes.

### 3.6 Undo / redo

- Every document change is one undoable step: add/remove/move/resize/recolour devices; add,
  remove, reorder, resize and rename racks; add/remove/move/rename ports; document rename;
  "New document".
- Selection and camera are **not** part of the history.
- A new change after undo clears the redo stack.
- History exists only for the session (not persisted).
- Importing a file clears the history.

## 4. Architecture

A Cargo workspace with three crates.

```
teststationview/
  crates/
    core/     pure Rust: model, rules, history, file format. No wasm/browser/GPU deps.
    render/   wgpu renderer, camera, picking, text textures.
    app/      Leptos (CSR) UI, interaction controller, storage, entry point.
```

### 4.1 `core`

- **Model** (all IDs are UUIDs, stable across renames and moves):
  ```
  Document { name: DocumentName, racks: Vec<Rack> }
  Rack     { id, name: Name, height_u: u32, devices: Vec<Device> }
  Device   { id, name: Name, bottom_u: u32, height_u: u32, color: Rgb,
             kind: DeviceKind, ports: Vec<Port> }
  Port     { id, name: Name, row: u32, col: u32, kind: PortKind }
  DeviceKind { AdHoc }          // reserved for a future catalog
  PortKind   { Unspecified }    // reserved for future connector types
  Limits   { name_max_len: 10, port_rows_per_u: 1, port_cols: 5, max_rack_height_u: 100 }
  ```
- **Newtypes:** `Name` and `DocumentName` can only be constructed through validation (§3.1, §3.2).
- **Plans:** each edit is a function that computes a complete candidate document without mutating
  the current one, e.g.
  `plan_device_drop(doc, source, target_rack, bottom_u) -> Result<Plan, Rejection>`.
  A `Plan` holds the resulting `Document` (including pushes and auto-renames). The app shows a plan
  as a live preview during a drag and commits it on drop. `Rejection` says why (for the red ghost
  and messages).
- **History:** undo/redo via whole-document snapshots. Documents are tiny, so storing copies is
  cheap and avoids writing an inverse for every operation.
- **Validation:** `validate(&Document, &Limits) -> Result<(), ValidationError>` checks every
  invariant (name rules, uniqueness, devices within racks and non-overlapping, ports within grids
  and non-overlapping, unique IDs). It is used on import and in tests.
- **File format:** serialized through dedicated file-format types, separate from the in-memory
  model, as JSON with a top-level `format_version` (1 in the MVP). Loading runs a migration chain
  `v1 → v2 → …` (empty in the MVP). A version newer than the app knows is rejected.

### 4.2 `render`

- **Backend:** wgpu, WebGPU backend only, drawing to the app's `<canvas>`.
- **Input:** a read-only scene description built each time something changes: the document (or the
  previewed plan's document), the selection, the hovered port, the drag ghost (geometry +
  valid/invalid) and the camera.
- **World units:** millimetres. 1U = 44.45 mm; device front width = 482.6 mm. Rack outer width
  (600 mm), rack depth, device depth, the header-panel height above the top U, and the inter-rack
  gap are constants in one module.
- **Look:** clean and schematic. Flat colours, one directional light plus ambient, no shadows.
  - Rack: simple frame (posts, top, bottom), U numbers on the rails, and a **header panel above the
    top U** (never holds equipment) with the rack name printed on it.
  - Device: a box in its colour with its name printed on the front face.
  - Port: a small marker in its cell. Its name appears beside/under it only while the port is
    hovered or selected, or while its device is selected (then all the device's port names show).
  - Floor: a faint 1 m grid at y = 0 under the racks, reaching 1.5 m beyond them and fading out
    towards its edge. It is hidden from below.
- **Geometry:** one unit-cube mesh drawn instanced (transform, colour and flags per instance).
- **Text:** strings are rasterised on an offscreen browser 2D canvas, uploaded as textures and
  drawn as quads just in front of the face. Text auto-scales to fit and is black or white depending
  on the background's luminance. Textures are cached per string and invalidated on change. The
  browser-dependent rasteriser sits behind a small trait so the rest of `render` stays testable
  natively.
- **Selection highlight:** outline glow. The selected object is drawn into a mask, and a
  full-screen pass draws a glow along the mask edge.
- **Ghost:** drawn translucent after opaque geometry. Device colour when the drop is valid,
  red and translucent when it would be rejected (with the other objects left in place).
- **Motion:** displayed positions of devices and ports interpolate to their target positions over
  about 120 ms. This animates push previews (and their reverting), commits and undo/redo.
- **Camera:** orbit state `{target, yaw, pitch, distance}`; pitch and distance clamped.
  - Left-drag on empty space orbits, middle-drag pans, the wheel zooms.
  - "Frame object" (tree double-click) and "Reset view" (front view of all racks) compute a camera
    that fits a bounding box.
  - **Port mode:** when a port drag starts (new or existing port), the current camera is saved and
    the camera tweens to a straight-on view of the device's front face. When the drag ends (drop,
    rejection or cancel) it tweens back to the saved camera.
  - All programmatic camera moves tween with easing over about 0.4 s.
- **Picking (CPU):** all geometry is axis-aligned. A cursor ray is tested against boxes and returns
  port / device / rack / empty, nearest first (ports before their device). Drag helpers:
  - Device drag: ray vs. the racks' front plane → target rack and U. Snapping keeps the device's
    grab point under the cursor (grabbed 1U above its bottom → its bottom stays 1U below the
    cursor).
  - Port drag: ray vs. the device's face plane → grid cell + offset from the cell centre.
- **Frame loop:** `requestAnimationFrame` runs only while something changes or animates.

### 4.3 `app`

#### Layout

```
┌───────────────────────────────────────────────────────────────┐
│ Toolbar: [New] [Import] [Export]  [Undo] [Redo]  [Reset view] │
├──────────────────┬────────────────────────────────────────────┤
│ Tree             │                                            │
│                  │            3D view (canvas)                │
│ [Add rack]       │                                            │
├──────────────────┤                                            │
│ Properties       │                                            │
├──────────────────┤                               ┌─────────┐  │
│ New device form  │                               │ Trash   │  │
│ New port form    │                               │ (drag   │  │
│                  │                               │  only)  │  │
└──────────────────┴────────────────────────────────────────────┘
```

#### Tree

- One tree: **Document → Rack → Device → Port**, expandable nodes.
- Racks in row order. Devices in physical order (highest U first). Ports in reading order.
- Single click selects the node and highlights the object in 3D.
- Double click frames the object in the camera.
- Context menu: Rename (inline edit in the tree) and Delete. The Document node has Rename only.
- Rack nodes can be dragged within the tree to reorder racks. Devices and ports can't be dragged in
  the tree.
- Selecting an object in 3D selects its node, expands its parents and scrolls it into view.

#### Selection

- Single selection of a document, rack, device or port, or nothing.
- Two-way between the tree and the 3D view.
- Left click (without movement) on an object in 3D selects it. On empty space, it clears the
  selection.
- Double click on an object in 3D selects it and frames it in the camera (as a tree double click
  does). Double click on empty space does nothing.

#### 3D context menu (right click)

- Rack, device or port: **Rename** (selects the object and focuses its name field in the
  properties panel) and **Delete** (racks with devices ask for confirmation).
- Empty space: no menu.

#### Properties panel

- Document: name.
- Rack: name, height (U).
- Device: name, height (U), colour (native colour input), position (read-only, e.g. `U12–U13`).
- Port: name.
- Text and number fields validate while typing and show errors inline. They commit on Enter or
  blur, and only when valid. Each commit is one undo step. Rejected edits (e.g. a rack height that
  would cut off equipment) show the reason and leave the document unchanged.
- The colour input commits on its `change` event (picker closed), not on every `input` event, so
  each colour change is one undo step.

#### New device form

- Fields: name, height (U, ≥ 1).
- The name is validated while typing. The drag handle is disabled until it's valid.
- Dragging the handle into the 3D view runs the device placement rules (§3.4) with live preview.
  Dropping commits, with auto-rename on a clash.

#### New port form

- Enabled when a device is selected, or when one of its ports is selected (that port's device is
  then the target). Shows the target device's name.
- The name is validated while typing, including uniqueness on the target device. The drag handle
  is disabled until it's valid.
- Starting the drag enters camera port mode (§4.2). The port can only be dropped on the target
  device's face. Anywhere else the drop is invalid.

#### Interaction controller

A state machine that owns all pointer and keyboard input:

```
Idle
 ├─ left-drag on empty space ─────────► Orbiting
 ├─ middle-drag ──────────────────────► Panning
 ├─ drag "new device" handle ─────────► DraggingNewDevice
 ├─ left-drag on a device ────────────► DraggingDevice
 ├─ drag "new port" handle ───────────► DraggingNewPort   (camera → port mode)
 └─ left-drag on a port ──────────────► DraggingPort      (camera → port mode)
```

- Drags use pointer events with pointer capture (not HTML5 drag-and-drop), so drags that start in
  the sidebar and drags that start in 3D share one code path.
- During a device/port drag, every pointer move: pick → `core` plan → render the preview or the
  invalid ghost.
- Release: over the trash zone → delete the object (for an existing object; a new one is simply
  discarded); otherwise commit a valid plan or discard a rejected one.
- **Esc**, or releasing where there is no valid target, cancels the drag with no change.
- Port drags always end with the camera tweening back.
- The trash zone (HTML overlay in a corner of the 3D view) is visible only during device/port drags
  and highlights while the pointer is over it.

#### Keyboard

- Undo: Ctrl+Z. Redo: Ctrl+Y and Ctrl+Shift+Z. These work only when no text field has focus
  (inside text fields, the browser's own text undo applies).
- Esc cancels an active drag.

#### Toolbar

- **New:** asks for confirmation, then replaces the document with the default (§3.1). This is
  undoable.
- **Import:** file picker → confirmation → load (§5).
- **Export:** downloads the document as `<document name>.json` (pretty-printed).
- **Undo / Redo:** disabled when there is nothing to undo/redo.
- **Reset view:** tweens to the default front view of all racks.

#### Dialogs

Confirmations and errors use an in-app modal, not the browser's `confirm()`/`alert()`.

## 5. Persistence

- **Interface:** in `app`:
  ```rust
  trait DocumentStore {
      async fn load(&self) -> Result<Option<Document>, StoreError>;
      async fn save(&self, doc: &Document) -> Result<(), StoreError>;
  }
  ```
  It is async so a future HTTP backend can implement it without changing callers. The MVP has
  only `LocalStorageStore` (one fixed key holding the file-format JSON).
- **Autosave:** after every commit, undo and redo.
- **Save failure** (storage disabled, quota): a non-blocking banner says "Changes can't be saved in
  this browser — use Export to keep your work." The app keeps working, and the banner clears after
  the next successful save.
- **Startup:**
  - Nothing stored → default document.
  - Stored document loads and validates → use it.
  - Stored data fails to load (corrupt, invalid or newer format) → a modal explains this and
    offers **Download the stored data** and **Start a new document**. Nothing overwrites the
    stored data until the user picks.
- **Import:** parse → migrate → `validate`. Any failure shows an error modal naming the first
  problem (e.g. "Rack 'Rack2': devices 'DMM' and 'PSU' overlap at U12"; or "This file was made by
  a newer version of the app."), and the current document stays untouched. On success: replace the
  document, clear history and selection, reset the view.
- **Multiple tabs:** each tab autosaves; the last write wins. No cross-tab synchronisation.

## 6. Error handling

- **No WebGPU** (`navigator.gpu` missing or no adapter): a full-page message saying the app requires
  WebGPU (Chrome, Edge or Firefox on Windows), instead of the app.
- **GPU device lost:** an overlay with a "Reload" button. The document is safe in local storage.
- **Rust panics:** logged to the browser console via a panic hook.
- **Rejected edits and drops:** never change the document. Drops show the red ghost; form edits
  show the reason inline.

## 7. Testing

- **`core`** (native `cargo test`):
  - Name normalisation and validation; document-name rules; case-insensitive uniqueness;
    auto-rename including truncation.
  - Device placement: free slot; push up; push down; centre-equal goes down; cascades in both
    directions; rejection at the top and below U1; clamping; moves within a rack (self excluded);
    moves between racks with auto-rename; device taller than the rack.
  - Height growth (push upward and rejection); shrink rejected when ports would fall off; rack
    height change rejected when equipment would fall outside.
  - Port push in all four directions; diagonal and exact-centre tie-breaks; cascade; rejection at
    the grid edge; moving a port into a cell freed by itself.
  - Undo/redo sequences; redo cleared by a new edit; history cleared on import.
  - File format round-trip; each validation error; rejection of newer versions.
- **`render`** (native): camera maths, framing, tweening, and picking (ray → hit, plane → rack/U,
  plane → cell + offset). Drawing itself is not tested automatically.
- **`app`** (`wasm-bindgen-test`, headless Chrome/Firefox, no GPU): `LocalStorageStore`
  round-trip, the import path including error cases, and interaction-controller state transitions
  driven by synthetic input.

## 8. Tooling

- Target `wasm32-unknown-unknown`; built and served with **Trunk** (`trunk serve` for development,
  `trunk build --release` for deployment, which produces static files).
- `rustfmt` and `clippy` for formatting and linting.
