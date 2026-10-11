mod common;

use common::*;
use rackwright_app::session::{DeleteOutcome, Session};
use rackwright_core::edit::ObjectId;
use rackwright_core::file_format::to_json;
use rackwright_core::model::{Document, Rgb};

fn names(s: &Session) -> Vec<String> {
    s.document()
        .racks
        .iter()
        .map(|r| r.name.to_string())
        .collect()
}

#[test]
fn edits_bump_revisions_and_are_undoable() {
    let mut s = session(Document::new_default(&limits()));
    let (rev, ui) = (s.revision(), s.ui_revision());
    s.add_rack();
    assert_eq!(names(&s), vec!["Rack1", "Rack2"]);
    assert!(s.revision() > rev && s.ui_revision() > ui);
    assert_eq!(
        s.selection(),
        Some(ObjectId::Rack(s.document().racks[1].id)),
        "new rack selected"
    );
    s.undo();
    assert_eq!(names(&s), vec!["Rack1"]);
    assert_eq!(
        s.selection(),
        None,
        "selection of a vanished object is cleared"
    );
    s.redo();
    assert_eq!(names(&s), vec!["Rack1", "Rack2"]);
}

#[test]
fn selecting_changes_only_the_ui_revision() {
    let mut s = session(Document::new_default(&limits()));
    let (rev, ui) = (s.revision(), s.ui_revision());
    let rack = ObjectId::Rack(s.document().racks[0].id);
    s.select(Some(rack));
    assert_eq!(s.revision(), rev);
    assert_eq!(s.ui_revision(), ui + 1);
    s.select(Some(rack));
    assert_eq!(s.ui_revision(), ui + 1, "no change, no bump");
}

#[test]
fn renaming_reports_errors_as_sentences() {
    let mut s = session(doc(vec![rack("A", 42, vec![]), rack("B", 42, vec![])]));
    let a = ObjectId::Rack(s.document().racks[0].id);
    assert_eq!(
        s.rename(a, "b"),
        Err("The name 'b' is already used here".into())
    );
    assert_eq!(s.rename(a, ""), Err("Name must not be empty".into()));
    assert_eq!(s.rename(a, "Lab 1"), Ok(()));
    assert_eq!(names(&s), vec!["Lab1", "B"]);
    assert_eq!(s.rename_document("My Station"), Ok(()));
    assert_eq!(s.document().name.as_str(), "My Station");
}

#[test]
fn heights_and_colours() {
    let d = device("D", 1, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let rid = s.document().racks[0].id;
    assert_eq!(
        s.set_rack_height(rid, "0"),
        Err("Height must be a whole number of at least 1".into())
    );
    assert_eq!(
        s.set_rack_height(rid, "101"),
        Err("The height must be at most 100U".into())
    );
    assert_eq!(s.set_rack_height(rid, "24"), Ok(()));
    assert_eq!(s.set_device_height(did, "3"), Ok(()));
    let red = Rgb { r: 255, g: 0, b: 0 };
    s.set_device_color(did, red);
    let (_, dev) = s.document().device(did).unwrap();
    assert_eq!((dev.height_u, dev.color), (3, red));
}

#[test]
fn deleting_a_rack_with_devices_needs_confirmation() {
    let mut s = session(doc(vec![
        rack("Full", 42, vec![device("D", 1, 1)]),
        rack("Empty", 42, vec![]),
    ]));
    let full = ObjectId::Rack(s.document().racks[0].id);
    let empty = ObjectId::Rack(s.document().racks[1].id);
    assert_eq!(s.request_delete(full), DeleteOutcome::NeedsConfirmation);
    assert_eq!(names(&s), vec!["Full", "Empty"]);
    assert_eq!(s.request_delete(empty), DeleteOutcome::Deleted);
    s.delete(full);
    assert!(s.document().racks.is_empty());
}

#[test]
fn new_document_is_undoable_and_import_clears_history() {
    let mut s = session(doc(vec![rack("Old", 42, vec![])]));
    s.new_document(0.0);
    assert_eq!(names(&s), vec!["Rack1"]);
    s.undo();
    assert_eq!(names(&s), vec!["Old"]);

    let imported = doc(vec![rack("Imp", 10, vec![])]);
    assert_eq!(s.import(&to_json(&imported), 0.0), Ok(()));
    assert_eq!(s.document(), &imported);
    assert!(!s.can_undo() && !s.can_redo());
}

#[test]
fn failed_import_leaves_everything_untouched() {
    let mut s = session(doc(vec![rack("Keep", 42, vec![])]));
    s.add_rack();
    let before = s.document().clone();
    let err = s.import("{\"format_version\": 99}", 0.0).unwrap_err();
    assert_eq!(err, "This file was made by a newer version of the app.");
    assert_eq!(s.document(), &before);
    assert!(s.can_undo());
}

#[test]
fn export_uses_the_document_name() {
    let mut s = session(Document::new_default(&limits()));
    s.rename_document("Lab 3").unwrap();
    let (file, json) = s.export();
    assert_eq!(file, "Lab 3.json");
    assert_eq!(json, s.save_payload());
}

#[test]
fn port_form_targets_the_selected_device_or_port_owner() {
    let p = port("P", 0, 0);
    let pid = p.id;
    let d = with_ports(device("D", 1, 1), vec![p]);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    assert_eq!(s.port_form_device(), None);
    s.select(Some(ObjectId::Device(did)));
    assert_eq!(s.port_form_device(), Some(did));
    s.select(Some(ObjectId::Port(pid)));
    assert_eq!(s.port_form_device(), Some(did));
    s.select(Some(ObjectId::Rack(s.document().racks[0].id)));
    assert_eq!(s.port_form_device(), None);
}

#[test]
fn reset_view_and_frame_tween_the_camera() {
    let d = device("D", 20, 2);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let start = *s.camera();
    let mut now = 1.0;
    s.frame(ObjectId::Device(did), now);
    settle(&mut s, &mut now);
    assert!(
        s.camera().distance < start.distance,
        "framing a device zooms in"
    );
    s.reset_view(now);
    settle(&mut s, &mut now);
    assert_eq!(*s.camera(), start);
}

#[test]
fn scene_reflects_selection() {
    let d = device("D", 1, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    assert!(s.scene().selected.is_empty());
    s.select(Some(ObjectId::Device(did)));
    assert_eq!(s.scene().selected.len(), 1);
}

#[test]
fn editing_an_object_that_no_longer_exists_is_an_error_not_a_panic() {
    let d = device("D", 1, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    s.delete(ObjectId::Device(did));
    assert_eq!(
        s.rename(ObjectId::Device(did), "X"),
        Err("The object no longer exists".into())
    );
    assert_eq!(
        s.set_device_height(did, "2"),
        Err("The object no longer exists".into())
    );
    s.set_device_color(did, Rgb::NEUTRAL_GREY);
    s.frame(ObjectId::Device(did), 0.0);
}

#[test]
fn a_port_type_change_is_one_undo_step() {
    use rackwright_core::model::PortKind;
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let mut s = session(doc(vec![rack(
        "R",
        42,
        vec![with_ports(device("D", 1, 1), vec![p])],
    )]));
    s.set_port_kind(pid, PortKind::Bnc);
    assert_eq!(s.document().port(pid).unwrap().2.kind, PortKind::Bnc);
    assert_eq!(s.revision(), 1);
    s.undo();
    assert_eq!(
        s.document().port(pid).unwrap().2.kind,
        PortKind::Unspecified
    );
}

#[test]
fn a_port_gender_change_is_one_undo_step() {
    use rackwright_core::model::Gender;
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let mut s = session(doc(vec![rack(
        "R",
        42,
        vec![with_ports(device("D", 1, 1), vec![p])],
    )]));
    s.set_port_gender(pid, Gender::Male);
    assert_eq!(s.document().port(pid).unwrap().2.gender, Gender::Male);
    assert_eq!(s.revision(), 1);
    s.undo();
    assert_eq!(
        s.document().port(pid).unwrap().2.gender,
        Gender::Unspecified
    );
}

#[test]
fn the_example_can_be_loaded_and_undone() {
    let mut s = session(doc(vec![rack("R", 42, vec![device("D", 1, 1)])]));
    let before = s.document().clone();
    s.load_example(0.0);
    assert_eq!(s.document().name.as_str(), "Example Station");
    assert_eq!(s.selection(), None);
    assert_eq!(s.revision(), 1);
    s.undo();
    assert_eq!(s.document(), &before);
}

#[test]
fn a_new_document_is_still_empty() {
    let mut s = session(rackwright_core::example::example_document(&limits()));
    s.new_document(0.0);
    let d = s.document();
    assert_eq!(d.name.as_str(), "Station1");
    assert_eq!(d.racks.len(), 1);
    assert!(d.racks[0].devices.is_empty() && d.cables.is_empty() && d.catalog.is_empty());
}
