# Future work

The MVP is a planning and documentation tool. The longer-term direction, set during the design
discussion, is a **digital twin of real test stations**: a shared, accurate record of what is
installed where and how it is connected, possibly showing live data from the instruments.

This page lists what was deliberately left out of the MVP and, where the code already prepares
for it, the extension point to build on. Nothing here has been designed in detail. Each item
needs its own design (spec, then plan) before implementation.

## Directions already prepared for in the code

### Equipment catalogue

**Done.** See `docs/superpowers/specs/2026-10-08-equipment-catalogue-design.md` and
`docs/superpowers/specs/2026-10-12-separate-catalogue-design.md`.

- **What exists:**
  - One catalogue of models (manufacturer, model, height, colour and default ports), kept
    apart from the station document. It is autosaved under its own key and exported and
    imported as its own file (Import replaces it).
  - "Save as model" makes an entry from a device. Dragging an entry into a rack places a copy
    linked to its model (`DeviceKind::Model`).
  - Changing an entry never changes devices already placed.
- **Still open:**
  - front-panel images;
  - a catalogue shared by the whole team (via the backend);
  - merging on Import catalogue, and several catalogues;
  - live links that push entry changes into placed devices;
  - a dedicated port-layout editor for entries.

### Connector types for ports

**Done.** See `docs/superpowers/specs/2026-10-08-port-types-design.md`. Ports have a type from a
fixed list (BNC, SMA, N-type, banana, USB, LAN, GPIB, D-sub, power, other), each with its own
marker shape and colour. A cable between two different specific types is allowed but flagged.

- **Adding a type:**
  - a `PortKind` variant (with `label`, `key` and `ALL`);
  - `FilePortKind` and a format-version bump;
  - `layout::marker_shape` and `scene::port_color`.
- **Still open:**
  - user-defined types;
  - an explicit adapter object instead of the mismatch warning.
- **Gender:** done (male, female or other/genderless; see
  `docs/superpowers/specs/2026-10-08-connector-gender-design.md`). It is shown by the marker's
  depth and never restricts cables.

### Cables between ports

**Done.** See `docs/superpowers/specs/2026-10-08-cables-design.md`. A cable joins two ports,
anywhere in the document, at most one per port. It has a name and a colour, and it is drawn in
3D as a sagging tube. Shift-dragging from a free port draws a cable; from a connected port it
re-plugs that cable's end.

- **Still open:**
  - cable type and length;
  - routing through cable trays, and rear-face ports;
  - several cables per port (splitters).
- **Extension points:**
  - `Cable` in `crates/core/src/model.rs` and its file-format twin `FileCable` (new fields need
    a format bump);
  - `layout::cable_path` for the shape;
  - the per-port rule in `plan_connect`, `plan_replug` and `validate::check_cables`.

### Shared backend storage

A server so the whole team works on the same documents instead of each browser's local
storage.

- **Extension point:** `DocumentStore` in `crates/app/src/storage.rs`. It is async on purpose,
  so an HTTP-backed store can replace `LocalStorageStore` without changing callers. It stores
  the document's JSON text. Startup recovery (`interpret_stored`) and autosave (`autosave`)
  already go through it.
- **Things the MVP does not handle yet:**
  - **More than one document:** local storage holds exactly one (`STORAGE_KEY`), next to the
    catalogue (`CATALOG_STORAGE_KEY`). Each goes through its own `DocumentStore`.
  - **Concurrent editing:** the MVP's rule is "last write wins", even between two tabs.
  - **Authentication.**
- **Also relevant:** IDs are UUIDs, so documents created in different browsers can be merged
  without ID clashes.

### Live data from instruments

Showing measurement status or values on the devices in 3D. Nothing is prepared for this
specifically. It would build on the backend, and probably on the catalogue (to know what each
instrument can report).

## Deliberately out of scope for the MVP

From the design spec (§2). These need design work before they can be added.

| Item | Notes / where it would touch |
|---|---|
| Half-width, fractional-height, configurable-depth or rear-mounted equipment | Placement in `core/src/placement.rs` is one-dimensional over whole U, and `Device` has no width, depth or side. This is the largest change: placement becomes 2D per side. |
| Rear-face ports | Ports are front-only (`Port` has no face). It needs a face field, a rear grid, and a rear camera view for port placement. |
| Free placement or rotation of racks, gaps between racks | Racks are laid out in a fixed row by `render/src/layout.rs::rack_left_x` (a fixed 40 mm gap). It needs rack positions in the model. |
| Multi-select | Selection is a single `Option<ObjectId>` in `Session`. |
| Several documents in local storage | See [Shared backend storage](#shared-backend-storage). |
| Cross-tab synchronisation | It could listen to the browser's `storage` event and reload or merge. |
| WebGL2 fallback | wgpu is built with WebGPU only. It needs wgpu's `webgl` feature and checks of shader and feature limits. |
| Automated visual tests of the 3D output | The renderer's offscreen pixel tests (`render/tests/gpu.rs`) are the starting point. |
| Hosting, CI pipeline, deployment automation | `trunk build --release` produces a static site in `crates/app/dist/`. Any static HTTPS host works. |

## Preliminary values meant to be tuned

These were marked "leave room for change" during the design and live in `Limits`
(`crates/core/src/limits.rs`), so changing them is a one-line edit plus tests:

- `name_max_len` = 10 characters (rack, device and port names).
- `port_rows_per_u` = 1 and `port_cols` = 5 (the port grid per U).
- `max_rack_height_u` = 100.

## Testing

Spec §7 planned a few `wasm-bindgen-test` tests in a headless browser. None were written,
because no browser could be installed in the development environment. Instead, all app logic
is tested natively, and the browser glue is covered by the manual checklist in plan 3, Task 6.
If a browser and driver become available (for example in CI), headless tests for
`LocalStorageStore` and the import path would close that gap.

## File format changes

Station files (`FORMAT_VERSION`) and catalogue files (`CATALOG_FORMAT_VERSION`) are versioned
separately. Any model change that affects saved files needs these three steps:

- Bump the matching version in `crates/core/src/file_format.rs`.
- Add a step to `migrate` that upgrades the previous version's JSON.
- Keep the on-disk `File*` types separate from the model.

Files from a newer version are already rejected with "This file was made by a newer version of
the app."
