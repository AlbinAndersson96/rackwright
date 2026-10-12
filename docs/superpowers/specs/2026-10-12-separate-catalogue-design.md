# Rackwright — Separate Equipment Catalogue

Date: 2026-10-12
Status: Accepted (decisions taken with the user in the design discussion)

This revises the equipment catalogue design (`2026-10-08-equipment-catalogue-design.md`). The
catalogue moves out of the station document into its own file. The app keeps one catalogue
for the browser, next to the current document, and it can be exported and imported on its own.

## 1. Decisions

| Question | Decision |
|---|---|
| Does a station document still carry its catalogue? | No. A station file holds racks, devices and cables only. Devices keep their link (`{ "model": "<uuid>" }`), which may point at a model that is not in the app's catalogue. |
| What does Import catalogue do? | It **replaces** the app's catalogue with the file's. |
| How are clashes resolved? | The imported entry wins, and devices stay linked. On a replace, a device in the current station whose model is not in the file by ID, but has the same manufacturer + model there, is relinked to the imported entry. Devices whose model is in neither are made ad hoc. |
| Undo | Catalogue edits share the document's undo history. The editor snapshots the document and the catalogue together. Import catalogue is one undo step. |
| Storage | The catalogue is autosaved under its own local-storage key, `rackwright.catalog`. |

## 2. Domain rules

The entry rules (text, identity, contents) are unchanged from the catalogue design §2. What
changes:

- **`Catalog`** is its own value: a list of `CatalogEntry`, with `model(id)`. `Document` no longer
  has a `catalog` field.
- **Links may dangle.** A device's `DeviceKind::Model(id)` need not name an entry in the current
  catalogue. This happens when a station file is opened next to a different catalogue. The
  device keeps its copied contents; the link is informational. The UI says "Model: not in
  catalogue" and offers no "Update model from this device" (it is rejected with "the model is
  not in the catalogue").
- **Save as model** and **Delete model** still change both: saving links the device, and deleting
  makes the current station's linked devices ad hoc. Both are one undo step.
- **Replace (Import catalogue):** the new catalogue is the file's. In the current station, each
  linked device is resolved in order:
  1. its model ID is in the file: unchanged;
  2. its model is in the old catalogue, and the file has an entry with the same manufacturer +
     model (case-insensitive): relinked to that entry;
  3. otherwise: made ad hoc.
- **Merge** (used by the example station and by legacy station files that still carry a
  catalogue, see §3): each incoming entry is matched against the app's catalogue by ID, then by
  manufacturer + model. A match takes the incoming contents and text but **keeps its ID**, so
  other stations linked to it stay linked; the incoming station is relinked to the kept ID. An
  unmatched entry is added. If an ID match leaves another entry with the same manufacturer +
  model, that other entry is removed.
- **New document** leaves the catalogue alone.
- **Example station** merges the example's models into the catalogue (opening it twice does not
  duplicate them) and replaces the document, as one undo step.
- **Import (station)** replaces the document and clears history, as before. The catalogue is
  unchanged, except for a legacy file's embedded catalogue, which is merged.

## 3. File formats

### Station file (version 6)

`FORMAT_VERSION` becomes 6. The `"catalog"` field is gone, and devices no longer have to link to
an existing entry.

- **Migration 5 → 6** removes `"catalog"`. The loader returns the removed entries alongside the
  document as a *legacy catalogue* (validated with the catalogue rules), so nothing saved before
  is lost. Versions 4 and 5 are the only ones that can carry one.
- **Importing a catalogue file as a station** is rejected with "This is a catalogue file. Use
  Import in the Catalogue panel."

### Catalogue file (version 1)

```json
{
  "kind": "rackwright-catalog",
  "format_version": 1,
  "entries": [ { "id", "manufacturer", "model", "height_u", "color", "ports": [ … ] } ]
}
```

- Entries have the same shape as version 5's embedded catalogue.
- `format_version` is versioned independently of station files (`CATALOG_FORMAT_VERSION`).
- Validation rejects duplicate IDs, duplicate manufacturer + model pairs, invalid text and
  invalid contents, as before.
- A file without `"kind": "rackwright-catalog"` is rejected. A station file gets "This is a
  station file, not a catalogue."
- Export names the file `catalogue.json`.

## 4. Storage and startup

- Two stored items: the document (`rackwright.document`) and the catalogue
  (`rackwright.catalog`). Both go through `DocumentStore`, one store per key. Every change saves
  both.
- **Startup:**
  - The document loads as before (a fresh visit opens the example station).
  - The catalogue loads from its key. With nothing stored, it is the stored document's legacy
    catalogue if it has one (saved right away), or empty. A fresh visit merges the example's
    models into it.
  - Each item that cannot be read is a separate startup problem: its stored data is offered for
    download, autosave does not overwrite that item until the user dismisses the problem, and
    the app starts with the default document or an empty catalogue.

## 5. Interaction and display

- **Catalogue panel:** **Import** and **Export** buttons in the panel header. Import asks first:
  "Replace the catalogue with the imported file? Devices linked to models that are not in the
  file become ad hoc. You can undo this."
- **Device properties:** a device whose model is missing shows "Model: not in catalogue". Save as
  model is still offered.
- The toolbar's Import and Export are for stations only.

## 6. Code shape

- **`rackwright-core`:**
  - `model::Catalog`; `Document::catalog` and `Document::model` removed.
  - `edit::CatalogPlan { document, catalog, subject }`. `plan_save_model`, `plan_update_model`,
    `plan_rename_model` and `plan_remove_model` take the catalogue and return a `CatalogPlan`.
    New: `plan_replace_catalog` and `merge_catalog`.
  - `DeviceSource::Model(CatalogEntry)`, so `plan_device_drop` needs no catalogue.
  - `Editor` holds both and snapshots both. `Editor::commit_catalog`.
  - `validate::validate_catalog`. Station validation no longer checks links.
  - `file_format`: format 6, `load_station` (document plus legacy catalogue),
    `catalog_to_json` and `catalog_from_json`.
  - `example::example_station` returns the document and its catalogue.
- **`rackwright-app`:** `Session::catalog`, `import_catalog` and `export_catalog`. Startup and
  autosave handle two stored items.
- **`rackwright-render`:** unchanged.

## 7. Out of scope

- Several catalogues, or choosing a catalogue per station.
- Merging on Import catalogue (only replace is offered).
- A backend-shared catalogue (`DocumentStore` remains the extension point).
