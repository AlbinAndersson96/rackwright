mod common;

use common::*;
use rackwright_core::edit::{plan_add_rack, plan_new_document};
use rackwright_core::editor::Editor;
use rackwright_core::model::Document;

fn add_rack(editor: &mut Editor) {
    let plan = plan_add_rack(editor.document(), &limits());
    editor.commit(plan);
}

fn editor() -> Editor {
    Editor::new(Document::new_default(&limits()))
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
    e.replace_clearing_history(imported.clone());
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
