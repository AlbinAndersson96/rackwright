mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::ids::RackId;
use rackwright_core::model::Document;
use rackwright_core::name::DocumentName;

#[test]
fn add_rack_appends_named_rack_with_default_height() {
    let d = Document::new_default(&limits());
    let plan = plan_add_rack(&d, &limits());
    let racks = &plan.document.racks;
    assert_eq!(rack_names(&plan.document), vec!["Rack1", "Rack2"]);
    assert_eq!(racks[1].height_u, 42);
    assert_eq!(plan.subject, Some(ObjectId::Rack(racks[1].id)));
    assert_eq!(d.racks.len(), 1, "planning must not mutate the input");
}

#[test]
fn add_rack_copies_height_of_last_rack() {
    let d = doc(vec![rack("Rack1", 42, vec![]), rack("Rack2", 24, vec![])]);
    let plan = plan_add_rack(&d, &limits());
    assert_eq!(plan.document.racks[2].name.as_str(), "Rack3");
    assert_eq!(plan.document.racks[2].height_u, 24);
}

#[test]
fn add_rack_uses_first_free_name_case_insensitively() {
    let d = doc(vec![rack("rack1", 42, vec![]), rack("Rack3", 42, vec![])]);
    let plan = plan_add_rack(&d, &limits());
    assert_eq!(plan.document.racks[2].name.as_str(), "Rack2");
}

#[test]
fn add_rack_to_empty_document_uses_defaults() {
    let d = doc(vec![rack("Rack1", 12, vec![])]);
    let emptied = plan_remove_rack(&d, d.racks[0].id).unwrap().document;
    let plan = plan_add_rack(&emptied, &limits());
    assert_eq!(rack_names(&plan.document), vec!["Rack1"]);
    assert_eq!(plan.document.racks[0].height_u, 42);
}

#[test]
fn remove_rack_removes_it_with_its_devices() {
    let d = doc(vec![
        rack("A", 42, vec![device("DMM", 1, 1)]),
        rack("B", 42, vec![]),
    ]);
    let plan = plan_remove_rack(&d, d.racks[0].id).unwrap();
    assert_eq!(rack_names(&plan.document), vec!["B"]);
    assert_eq!(plan.subject, None);
}

#[test]
fn unknown_rack_is_not_found() {
    let d = Document::new_default(&limits());
    assert_eq!(
        plan_remove_rack(&d, RackId::new()).unwrap_err(),
        Rejection::NotFound
    );
    assert_eq!(
        plan_move_rack(&d, RackId::new(), 0).unwrap_err(),
        Rejection::NotFound
    );
    assert_eq!(
        plan_set_rack_height(&d, &limits(), RackId::new(), 10).unwrap_err(),
        Rejection::NotFound
    );
}

#[test]
fn move_rack_reorders_the_row() {
    let d = doc(vec![
        rack("A", 42, vec![]),
        rack("B", 42, vec![]),
        rack("C", 42, vec![]),
    ]);
    let plan = plan_move_rack(&d, d.racks[2].id, 0).unwrap();
    assert_eq!(rack_names(&plan.document), vec!["C", "A", "B"]);
    let plan = plan_move_rack(&d, d.racks[0].id, 99).unwrap();
    assert_eq!(rack_names(&plan.document), vec!["B", "C", "A"]);
}

#[test]
fn rename_rack_rejects_clash_case_insensitively() {
    let d = doc(vec![rack("Rack1", 42, vec![]), rack("Rack2", 42, vec![])]);
    assert_eq!(
        plan_rename_rack(&d, d.racks[0].id, name("rack2")).unwrap_err(),
        Rejection::NameTaken("rack2".into())
    );
}

#[test]
fn rename_rack_allows_changing_case_of_its_own_name() {
    let d = doc(vec![rack("Rack1", 42, vec![])]);
    let plan = plan_rename_rack(&d, d.racks[0].id, name("RACK1")).unwrap();
    assert_eq!(rack_names(&plan.document), vec!["RACK1"]);
}

#[test]
fn set_rack_height_rejects_cutting_off_equipment() {
    let d = doc(vec![rack("R", 42, vec![device("PSU", 40, 2)])]);
    let id = d.racks[0].id;
    assert_eq!(
        plan_set_rack_height(&d, &limits(), id, 41)
            .unwrap()
            .document
            .racks[0]
            .height_u,
        41
    );
    assert_eq!(
        plan_set_rack_height(&d, &limits(), id, 40).unwrap_err(),
        Rejection::EquipmentOutside
    );
}

#[test]
fn set_rack_height_rejects_zero() {
    let d = doc(vec![rack("R", 42, vec![])]);
    assert_eq!(
        plan_set_rack_height(&d, &limits(), d.racks[0].id, 0).unwrap_err(),
        Rejection::ZeroHeight
    );
}

#[test]
fn rename_document() {
    let d = Document::new_default(&limits());
    let plan = plan_rename_document(&d, DocumentName::parse("Lab 3").unwrap());
    assert_eq!(plan.document.name.as_str(), "Lab 3");
}

#[test]
fn new_document_plan_is_the_default_document() {
    let plan = plan_new_document(&limits());
    assert_eq!(plan.document.name.as_str(), "Station1");
    assert_eq!(rack_names(&plan.document), vec!["Rack1"]);
    assert_eq!(plan.document.racks[0].height_u, 42);
}

#[test]
fn rejection_messages_are_sentences() {
    assert_eq!(
        Rejection::NameTaken("DMM".into()).to_string(),
        "the name 'DMM' is already used here"
    );
}

#[test]
fn set_rack_height_rejects_heights_above_the_limit() {
    let d = doc(vec![rack("R", 42, vec![])]);
    let id = d.racks[0].id;
    assert_eq!(
        plan_set_rack_height(&d, &limits(), id, 100)
            .unwrap()
            .document
            .racks[0]
            .height_u,
        100
    );
    assert_eq!(
        plan_set_rack_height(&d, &limits(), id, 101).unwrap_err(),
        Rejection::HeightAboveLimit { max: 100 }
    );
}
