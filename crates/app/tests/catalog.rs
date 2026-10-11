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
    let entry = &s.document().catalog[0];
    assert_eq!(entry.display_name(), "Keysight 34465A");
    assert_eq!(
        s.document().racks[0].devices[0].kind,
        DeviceKind::Model(entry.id)
    );
    assert_eq!(s.selection(), Some(ObjectId::Device(dmm)));
    s.undo();
    assert!(s.document().catalog.is_empty());
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
    let id = s.document().catalog[0].id;
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
    let id = s.document().catalog[0].id;
    assert_eq!(s.rename_model(id, "Agilent", "34401A"), Ok(()));
    assert_eq!(s.document().catalog[0].display_name(), "Agilent 34401A");
    s.remove_model(id);
    assert!(s.document().catalog.is_empty());
    assert_eq!(s.document().racks[0].devices[0].kind, DeviceKind::AdHoc);
    s.undo();
    assert_eq!(s.document().catalog.len(), 1);
}
