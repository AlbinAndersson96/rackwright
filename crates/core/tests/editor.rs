mod common;

use common::*;
use rackwright_core::edit::{plan_add_rack, plan_new_document};
use rackwright_core::editor::Editor;
use rackwright_core::model::{Catalog, Document};

fn add_rack(editor: &mut Editor) {
    let plan = plan_add_rack(editor.document(), &limits());
    editor.commit(plan);
}

fn editor() -> Editor {
    Editor::new(Document::new_default(&limits()), Catalog::default())
}

#[test]
fn commit_undo_and_redo() {
    let mut e = editor();
    add_rack(&mut e);
    assert_eq!(rack_names(e.document()), vec!["Rack1", "Rack2"]);
    assert!(e.can_undo());

    assert!(e.undo());
    assert_eq!(rack_names(e.document()), vec!["Rack1"]);
    assert!(e.can_redo());

    assert!(e.redo());
    assert_eq!(rack_names(e.document()), vec!["Rack1", "Rack2"]);
}

#[test]
fn a_new_commit_clears_redo() {
    let mut e = editor();
    add_rack(&mut e);
    e.undo();
    add_rack(&mut e);
    assert!(!e.can_redo());
    assert!(!e.redo());
}

#[test]
fn undo_and_redo_on_empty_history_do_nothing() {
    let mut e = editor();
    let before = e.document().clone();
    assert!(!e.undo());
    assert!(!e.redo());
    assert_eq!(e.document(), &before);
}

#[test]
fn replacing_the_document_clears_history() {
    let mut e = editor();
    add_rack(&mut e);
    add_rack(&mut e);
    e.undo();
    let imported = doc(vec![rack("Imported", 12, vec![])]);
    e.replace_clearing_history(imported.clone(), Catalog::default());
    assert_eq!(e.document(), &imported);
    assert!(!e.can_undo());
    assert!(!e.can_redo());
}

#[test]
fn new_document_is_undoable() {
    let mut e = editor();
    add_rack(&mut e);
    e.commit(plan_new_document(&limits()));
    assert_eq!(rack_names(e.document()), vec!["Rack1"]);
    e.undo();
    assert_eq!(rack_names(e.document()), vec!["Rack1", "Rack2"]);
}

#[test]
fn catalogue_edits_undo_together_with_the_document() {
    let mut e = editor();
    let device = rackwright_core::edit::plan_device_drop(
        e.document(),
        &limits(),
        &rackwright_core::edit::DeviceSource::New {
            name: name("DMM"),
            height_u: 1,
        },
        e.document().racks[0].id,
        1,
    )
    .unwrap();
    e.commit(device);
    let id = e.document().racks[0].devices[0].id;
    let plan = rackwright_core::edit::plan_save_model(
        e.document(),
        e.catalog(),
        id,
        rackwright_core::name::ModelText::parse("").unwrap(),
        rackwright_core::name::ModelText::parse("34465A").unwrap(),
    )
    .unwrap();
    e.commit_catalog(plan);
    assert_eq!(e.catalog().entries.len(), 1);

    // A document edit afterwards keeps the catalogue.
    add_rack(&mut e);
    assert_eq!(e.catalog().entries.len(), 1);
    e.undo();
    e.undo();
    assert!(e.catalog().entries.is_empty());
    assert_eq!(
        e.document().racks[0].devices[0].kind,
        rackwright_core::model::DeviceKind::AdHoc
    );
    e.redo();
    assert_eq!(e.catalog().entries.len(), 1);
}
