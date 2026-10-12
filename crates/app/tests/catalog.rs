mod common;

use common::*;
use glam::Vec3;
use rackwright_app::interaction::DragSource;
use rackwright_core::edit::ObjectId;
use rackwright_core::model::{DeviceKind, Rgb};
use rackwright_render::layout::{RACK_WIDTH_MM, U_MM, rack_left_x, u_bottom_y};

/// A point on rack 0's front plane at the middle of unit `u`.
fn unit_point(u: u32) -> Vec3 {
    let x = rack_left_x(0) + RACK_WIDTH_MM / 2.0;
    Vec3::new(x, u_bottom_y(u) + U_MM / 2.0, 0.0)
}

fn station() -> rackwright_app::session::Session {
    let mut dmm = with_ports(device("DMM", 10, 2), vec![port("HI", 0, 0)]);
    dmm.color = Rgb { r: 200, g: 0, b: 0 };
    session(doc(vec![rack("R1", 42, vec![dmm])]))
}

#[test]
fn saving_a_model_links_the_device_and_is_undoable() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    assert_eq!(s.save_model(dmm, " Keysight ", "34465A"), Ok(()));
    let entry = &s.catalog().entries[0];
    assert_eq!(entry.display_name(), "Keysight 34465A");
    assert_eq!(
        s.document().racks[0].devices[0].kind,
        DeviceKind::Model(entry.id)
    );
    assert_eq!(s.selection(), Some(ObjectId::Device(dmm)));
    s.undo();
    assert!(s.catalog().entries.is_empty());
}

#[test]
fn catalogue_errors_are_sentences() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    assert_eq!(
        s.save_model(dmm, "Keysight", ""),
        Err("The model must not be empty".to_string())
    );
    assert_eq!(
        s.save_model(dmm, &"x".repeat(41), "A"),
        Err("Text must be at most 40 characters".to_string())
    );
    s.save_model(dmm, "Keysight", "34465A").unwrap();
    assert_eq!(
        s.save_model(dmm, "keysight", "34465a"),
        Err("The model 'keysight 34465a' is already in the catalogue".to_string())
    );
    assert_eq!(s.update_model(s.document().racks[0].devices[0].id), Ok(()));
    assert_eq!(s.revision(), 2);
}

#[test]
fn dragging_a_model_into_a_rack_places_a_copy() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    s.save_model(dmm, "Keysight", "34465A").unwrap();
    let id = s.catalog().entries[0].id;
    s.start_drag(DragSource::Model(id), 0.0);
    assert!(s.is_dragging());
    let p = px(&s, unit_point(30));
    s.pointer_move(p, false, 0.0);
    let ghost = s.ghost().expect("ghost");
    assert!(ghost.valid);
    assert_eq!(ghost.color, Rgb { r: 200, g: 0, b: 0 });
    assert!(
        (ghost.aabb.size().y - 2.0 * U_MM).abs() < 1e-3,
        "two units tall"
    );
    s.pointer_up(p, false, 0.0);

    let devices = &s.document().racks[0].devices;
    assert_eq!(devices.len(), 2);
    let placed = devices.iter().find(|d| d.id != dmm).unwrap();
    assert_eq!(placed.name.as_str(), "34465A");
    assert_eq!(placed.kind, DeviceKind::Model(id));
    assert_eq!(placed.ports.len(), 1);
    assert_eq!(s.selection(), Some(ObjectId::Device(placed.id)));
}

#[test]
fn renaming_and_removing_models() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    s.save_model(dmm, "Keysight", "34465A").unwrap();
    let id = s.catalog().entries[0].id;
    assert_eq!(s.rename_model(id, "Agilent", "34401A"), Ok(()));
    assert_eq!(s.catalog().entries[0].display_name(), "Agilent 34401A");
    s.remove_model(id);
    assert!(s.catalog().entries.is_empty());
    assert_eq!(s.document().racks[0].devices[0].kind, DeviceKind::AdHoc);
    s.undo();
    assert_eq!(s.catalog().entries.len(), 1);
}

#[test]
fn the_catalogue_is_exported_and_imported_on_its_own() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    s.save_model(dmm, "Keysight", "34465A").unwrap();
    let (file, json) = s.export_catalog();
    assert_eq!(file, "catalogue.json");
    assert_eq!(json, s.catalog_payload());
    assert!(
        !s.export().1.contains("34465A"),
        "stations carry no catalogue"
    );

    // Into a session with another model, linked from a device: replaced, and that device
    // becomes ad hoc. One undo step brings both back.
    let mut other = station();
    let theirs = other.document().racks[0].devices[0].id;
    other.save_model(theirs, "Other", "Box").unwrap();
    let before = (other.document().clone(), other.catalog().clone());
    let revision = other.revision();
    assert_eq!(other.import_catalog(&json), Ok(()));
    assert_eq!(other.catalog(), s.catalog());
    assert_eq!(other.document().racks[0].devices[0].kind, DeviceKind::AdHoc);
    assert!(other.revision() > revision, "autosave runs");
    other.undo();
    assert_eq!((other.document().clone(), other.catalog().clone()), before);
}

#[test]
fn importing_the_wrong_kind_of_file_says_so() {
    let mut s = station();
    let (_, station_json) = s.export();
    let (_, catalog_json) = s.export_catalog();
    assert_eq!(
        s.import_catalog(&station_json),
        Err("This is a station file, not a catalogue.".to_string())
    );
    assert_eq!(
        s.import(&catalog_json, 0.0),
        Err("This is a catalogue file. Use Import in the Catalogue panel.".to_string())
    );
    assert!(!s.can_undo());
}

#[test]
fn a_device_whose_model_is_missing_cannot_update_it() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    s.save_model(dmm, "Keysight", "34465A").unwrap();
    let (_, json) = s.export();
    let mut fresh = session(doc(vec![]));
    fresh.import(&json, 0.0).unwrap();
    assert!(fresh.catalog().entries.is_empty());
    assert!(matches!(
        fresh.document().racks[0].devices[0].kind,
        DeviceKind::Model(_)
    ));
    assert_eq!(
        fresh.update_model(dmm),
        Err("The model is not in the catalogue".to_string())
    );
}

#[test]
fn importing_an_older_station_merges_its_catalogue() {
    let mut s = station();
    let dmm = s.document().racks[0].devices[0].id;
    s.save_model(dmm, "Keysight", "34465A").unwrap();
    let mut v5: serde_json::Value = serde_json::from_str(&s.export().1).unwrap();
    let catalog: serde_json::Value = serde_json::from_str(&s.catalog_payload()).unwrap();
    v5["format_version"] = 5.into();
    v5["catalog"] = catalog["entries"].clone();

    // The receiving session already has the same model under another ID.
    let mut other = station();
    let theirs = other.document().racks[0].devices[0].id;
    other.save_model(theirs, "keysight", "34465a").unwrap();
    let kept = other.catalog().entries[0].id;
    other.import(&v5.to_string(), 0.0).unwrap();
    assert_eq!(other.catalog().entries.len(), 1);
    assert_eq!(other.catalog().entries[0].id, kept);
    assert_eq!(other.catalog().entries[0].display_name(), "Keysight 34465A");
    assert_eq!(
        other.document().racks[0].devices[0].kind,
        DeviceKind::Model(kept)
    );
}
