# TestStationView — Cables Between Ports

Date: 2026-10-08
Status: Accepted (decisions taken with the user in the design discussion)

This extends the MVP design (`2026-10-05-rackwright-mvp-design.md`). Everything not
mentioned here behaves as described there. It answers the open questions in
`docs/future-work.md` § "Cables between ports".

## 1. Decisions

| Question | Decision |
|---|---|
| Shown how? | Drawn in 3D and listed in the tree. |
| Cables per port | At most one. Connecting a port that already has a cable is rejected. |
| Attributes | A name and a colour. Type and length are left for later. |
| Created how? | By dragging from one port to another in the 3D view. |
| Across racks? | Yes. A cable can connect any two ports in the document, also on the same device. |
| Port or device removed? | Its cables are removed with it, in the same undo step. |
| Routing | None. A cable is a smooth curve that leaves both ports towards the viewer and sags. |

## 2. Domain rules

- A **cable** has a stable ID (`CableId`), a **name**, a **colour** and two **ends** (`a`, `b`),
  each a `PortId`.
- **Name:** the rules for rack, device and port names (spec §3.2). It is unique within the
  document, compared case-insensitively. A new cable is named after the first free `CableN`
  (N ≥ 1). The name can be changed afterwards.
- **Colour:** any `#rrggbb`. A new cable gets the default cable colour (`#2f6fd6`).
- **Ends:** both must be existing ports, and they must be different ports. No port may be the
  end of more than one cable. The order of `a` and `b` carries no meaning.
- **Connect** is rejected with a sentence when:
  - either port no longer exists ("The object no longer exists");
  - both ends are the same port ("A cable needs two different ports");
  - either port already has a cable ("Port 'X' already has a cable").
- Removing a port, a device or a rack also removes every cable with an end on a removed port.
  Moving a port or a device (also into another rack) keeps its cables. They follow the port.
- New document: no cables. Import: cables come from the file.

## 3. File format (version 2)

`FORMAT_VERSION` becomes 2. The document gains a `cables` array:

```json
"cables": [
  { "id": "…uuid…", "name": "Cable1", "color": "#2f6fd6", "a": "…port uuid…", "b": "…port uuid…" }
]
```

- **Migration 1 → 2** adds `"cables": []`.
- Version-2 files without `cables` are malformed (the field is required).
- `validate` adds these checks:
  - cable IDs are unique together with all other IDs;
  - cable names are unique;
  - both ends exist and differ;
  - no port is used by two cables.

## 4. 3D view

- **Look:** a cable is a tube, 7 mm thick, in its colour. The path is a cubic Bézier curve. It
  starts at the front centre of each end's port marker. Both inner control points are pulled
  out of the rack front by a distance that grows with the span (`clamp(span × 0.35, 60, 300)` mm)
  and lowered by a sag (`span × 0.2`). The curve is drawn as 24 straight tube segments. A
  cable's ends follow its ports' eased motion, so cables move smoothly with pushed ports and
  devices.
- **Selection:** a selected cable gets the orange selection outline, like any other object.
- **Picking:** the ray is tested against every tube segment, with a pick radius of 2× the tube
  radius so thin cables are easy to hit. The nearest hit wins, as for other objects, except that
  a port marker under the pointer always beats a cable. A cable starts right in front of its
  ports, so otherwise a connected port could hardly be clicked or moved.
- **Port names:** while a cable is selected, the names of both its ports are shown.
- **Framing** (double-click, tree double-click) fits the cable's path.

## 5. Interaction

- **Shift + left-drag from a port** draws a cable. A plain left-drag from a port still moves the
  port, as before.
  - **While dragging:** a preview tube runs from the port to the pointer.
    - Over another port, the preview shows the planned cable, ending at that port's centre, and
      that port's name is shown.
    - Over a port that already has a cable, the preview runs to that port and is red.
    - Over the start port itself, the preview is a short loop out of the port and back (not red,
      since the pointer has simply not left the port yet).
    - Over anything else, the preview follows the point under the pointer: the surface it hits,
      or the front plane.
  - **Release** over a valid port creates the cable (one undo step) and selects it. Release
    anywhere else changes nothing.
  - **Esc**, pointer cancel or losing the window cancels the drag. The trash zone is not shown
    for cable drags.
  - The camera does not move into port mode.
- **Shift + left-drag from a port that has a cable** re-plugs that cable's end (added
  2026-10-08, after the first release).
  - **Pick-up:** the end at that port comes loose. The cable keeps its other end, name and
    colour, and the loose end follows the pointer as a preview in the cable's own colour.
  - **Over a free port:** the preview shows the cable plugged in there, and that port's name is
    shown.
  - **Over a port that cannot take it** (a port with another cable, or the cable's other end),
    the preview runs to that port and is red.
  - **Over the port it came from:** the cable is shown as it was.
  - **Over anything else:** the loose end follows the point under the pointer.
  - **Release:** over a free port, the end moves there in one undo step and the cable is
    selected. Over the trash zone, which is shown for these drags, the cable is deleted.
    Anywhere else, nothing changes.
  - **Cancel:** Esc, pointer cancel or losing the window, as for other drags.
  - **Rule:** re-plugging is rejected when the target port already has a cable, when it is the
    cable's other end, or when the cable or a port no longer exists. Re-plugging an end onto
    the port it is already on changes nothing.
- **Properties panel:**
  - **Port:** shows its cable's name (a click selects the cable). A port without a cable shows
    a "⠿ Drag to a port to connect" handle that starts the same drag. A port with a cable shows
    a "⠿ Drag to re-plug" handle that picks up the cable's end at this port.
  - **Cable:** Name (editable), Colour (editable), and both ends as `Rack/Device/Port`, read-only.
- **Tree:** a "Cables" list under the racks, with one row per cable. The label is the cable's
  name and the detail is its ends as `Device.Port – Device.Port`. Click selects, double-click
  frames, and right-click offers Rename and Delete, like other rows. A port's row shows its
  cable's name as its detail.
- **Context menu in 3D** on a cable: Rename and Delete, as for other objects.
- **Help overlay:** adds "Shift-drag a port: connect or re-plug a cable".

## 6. Code shape

- **`tsv-core`:**
  - `CableId`, `Cable`, `Document::cables` and the lookups `cable` and `cable_at_port`.
  - `ObjectId::Cable`.
  - `edit/cable.rs`: `plan_connect`, `plan_replug`, `plan_remove_cable`, `plan_rename_cable`
    and `plan_set_cable_color`.
  - The removal plans for ports, devices and racks drop the attached cables.
  - Format v2 with migration, and the new validation errors.
- **`tsv-render`:**
  - `layout::port_anchor` and `layout::cable_path` (pure).
  - `scene::TubeInstance`, plus `Scene::tubes` and `Scene::selected_tubes`.
  - `SceneInput::cable_preview` (with the preview's colour) and `SceneInput::hidden_cable`
    (the cable whose end is being re-plugged is drawn only as the preview).
  - `pick` handles cables (ray–segment distance).
  - `object_bounds` handles cables.
  - `gpu` gets a tube pipeline (an 8-sided prism with end caps, unlit edges) and a tube mask
    pipeline for the selection outline.
- **`tsv-app`:**
  - `DragSource::Cable { from }`, `DragSource::CableEnd { cable, end }` and `pointer_down`'s
    modifier flag.
  - Session methods to rename, recolour and delete cables (through the existing `rename` and
    `delete`) and `set_cable_color`.
  - UI: properties, tree list, help line and handle.

## 7. Out of scope (still)

- Cable type, length, connector compatibility, routing through cable trays, and rear ports.
- Several cables per port (splitters, daisy chains).
