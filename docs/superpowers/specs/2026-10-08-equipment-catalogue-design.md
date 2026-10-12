# TestStationView — Equipment Catalogue

Date: 2026-10-08
Status: Accepted (decisions taken with the user in the design discussion). Revised by
`2026-10-12-separate-catalogue-design.md`: the catalogue now lives in its own file, not in each
document.

This extends the MVP design, the cables design and the port types design. It answers
`docs/future-work.md` § "Equipment catalogue".

## 1. Decisions

| Question | Decision |
|---|---|
| Where is the catalogue? | In each document. It is exported and imported with it. |
| What is an entry? | Manufacturer, model, height, default colour and default ports (name, cell and type of each). No images. |
| Link to placed devices | Copy on placement, keep a link. Placing copies height, colour and ports into the new device, which stays freely editable and remembers its model. Changing an entry never changes devices already placed. |
| Editing | "Save as model" makes an entry from a device. A Catalogue panel lists the entries, where you can rename, delete and drag them into a rack. |

## 2. Domain rules

- **Entry (`CatalogEntry`):**
  - `id` (`ModelId`);
  - `manufacturer` (text, may be empty);
  - `model` (text, required);
  - `height_u`;
  - `color`;
  - `ports`: a list of name, row, column and type.
- **Text fields:**
  - Leading and trailing whitespace is trimmed, and inner spaces are allowed.
  - At most 40 characters, with no control characters.
  - The model must not be empty.
  - Invalid input is rejected with a sentence.
- **Identity:** manufacturer + model is unique in the catalogue, compared case-insensitively.
  "Keysight 34465A" and "keysight 34465a" are the same entry.
- **Display name:** "Manufacturer Model", or only the model when the manufacturer is empty.
- **Entry contents** obey the device rules:
  - the height is between 1 and `max_rack_height_u`;
  - every port lies inside the height's port grid;
  - port names are unique;
  - no two ports share a cell.
- **Link:** a device's `kind` is `AdHoc` or `Model(ModelId)`. A linked device's model must
  exist in the catalogue.
- **Save as model** (from a device):
  - Creates an entry with the device's height, colour and ports.
  - Links the device to the new entry.
  - Rejected when the manufacturer + model already exists.
- **Update model from this device** (linked devices only): replaces the entry's height, colour
  and ports with the device's. Other placed devices are not changed.
- **Rename model:** changes the manufacturer and model, with the same uniqueness rule.
- **Delete model:** removes the entry, and the devices linked to it become ad hoc. Nothing else
  about them changes. This is one undo step, with no confirmation.
- **Placing a model:** dropping an entry into a rack is a device drop (same push and clamp
  rules as a new device).
  - The new device copies the entry's height, colour and ports. The ports get new IDs.
  - The device is linked to the entry.
  - Its **name** comes from the model text: whitespace removed, cut to `name_max_len`, and
    "Device" if nothing is left. On a clash it is auto-renamed (spec §3.2), as for other
    devices entering a rack.

## 3. File format (version 4)

`FORMAT_VERSION` becomes 4.

- The document gains `"catalog": [ … ]`. Each entry has `id`, `manufacturer`, `model`,
  `height_u`, `color` and `ports`, where each port is `{ name, row, col, kind }`.
- A device's `kind` is `"ad_hoc"` or `{ "model": "<uuid>" }`.
- **Migration 3 → 4** adds `"catalog": []`.
- **Validation** rejects:
  - duplicate entry IDs (shared with all other IDs);
  - duplicate manufacturer + model pairs;
  - invalid entry text or contents;
  - devices linked to a missing entry.

## 4. Interaction and display

- **Catalogue panel** (sidebar, below New device):
  - **Rows:** one per entry, sorted by display name, showing "Keysight 34465A", then
    "2U · 3 ports".
  - **Placing:** each row has a "⠿" drag handle. Dragging it into a rack places the model,
    with a ghost in the entry's height and colour.
  - **Editing:** clicking a row opens Manufacturer and Model fields (commit on Enter or blur,
    errors inline) and a Delete button.
  - **Empty catalogue:** shows "Select a device and use Save as model".
- **Device properties:**
  - **Linked device:** shows "Model: <display name>" and an "Update model from this device"
    button.
  - **Every device:** has a "Save as model" section with Manufacturer and Model fields and a
    Save button, pre-filled with the linked model's text. Saving selects the device, which is
    now linked.
- **Tree:** a linked device's detail shows its model after the position, as in
  "U10 · 34465A".

## 5. Code shape

- **`tsv-core`:**
  - `ModelId`, `CatalogEntry`, `ModelPort`, `ModelText`, `Document::catalog`,
    `DeviceKind::Model`, and `Document::model`.
  - `edit/catalog.rs`: `plan_save_model`, `plan_update_model`, `plan_rename_model` and
    `plan_remove_model`.
  - `DeviceSource::Model(ModelId)` in `plan_device_drop`.
  - Validation, and format v4.
- **`tsv-app`:**
  - `DragSource::Model(ModelId)`.
  - The `Session` methods `save_model`, `update_model`, `rename_model` and `remove_model`.
  - Catalogue panel and properties UI.
- **`tsv-render`:** unchanged.

## 6. Out of scope

- Front-panel images.
- A shared catalogue across documents. Entries travel with each file; a backend could later
  offer a shared one through `DocumentStore`.
- Live links that push entry changes into placed devices.
- A dedicated port-layout editor for entries. Edit a device, then use "Update model from this
  device".
