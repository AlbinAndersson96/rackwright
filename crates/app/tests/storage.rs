mod common;

use std::cell::RefCell;

use common::*;
use rackwright_app::session::{Session, StartupProblem, StoredItem};
use rackwright_app::storage::*;
use rackwright_core::file_format::{catalog_to_json, to_json};
use rackwright_core::model::{Catalog, DeviceKind, Document};

fn none() -> Result<Option<String>, String> {
    Ok(None)
}

#[test]
fn nothing_stored_starts_with_the_example() {
    let s = interpret_stored(none(), none(), &limits());
    assert_eq!(s.document.name.as_str(), "Example Station");
    assert!(!s.document.cables.is_empty() && !s.catalog.entries.is_empty());
    assert!(s.problems.is_empty());
    assert!(!s.save_now);
}

#[test]
fn unavailable_storage_starts_fresh() {
    let s = interpret_stored(Err("blocked".into()), Err("blocked".into()), &limits());
    assert_eq!(s.document.name.as_str(), "Example Station");
    assert!(s.problems.is_empty());
}

#[test]
fn a_first_visit_merges_the_example_into_a_stored_catalogue() {
    let (_, models) = rackwright_core::example::example_station(&limits());
    let s = interpret_stored(none(), Ok(Some(catalog_to_json(&models))), &limits());
    assert_eq!(s.catalog, models, "the same models, under the stored IDs");
    let linked = s
        .document
        .racks
        .iter()
        .flat_map(|r| &r.devices)
        .filter_map(|d| match d.kind {
            DeviceKind::Model(m) => Some(m),
            DeviceKind::AdHoc => None,
        });
    for m in linked {
        assert!(models.model(m).is_some());
    }
}

#[test]
fn stored_document_and_catalogue_are_restored() {
    let d = doc(vec![rack("Lab", 12, vec![device("DMM", 1, 1)])]);
    let (_, c) = rackwright_core::example::example_station(&limits());
    let s = interpret_stored(
        Ok(Some(to_json(&d))),
        Ok(Some(catalog_to_json(&c))),
        &limits(),
    );
    assert_eq!((s.document, s.catalog), (d, c));
    assert!(s.problems.is_empty() && !s.save_now);
}

/// A version-5 document with `c` embedded, as stored before the catalogue moved out.
fn version_5(d: &Document, c: &Catalog) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&to_json(d)).unwrap();
    let entries: serde_json::Value = serde_json::from_str(&catalog_to_json(c)).unwrap();
    value["format_version"] = 5.into();
    value["catalog"] = entries["entries"].clone();
    value.to_string()
}

#[test]
fn an_older_stored_document_hands_over_its_catalogue() {
    let d = doc(vec![rack("Lab", 12, vec![])]);
    let (_, c) = rackwright_core::example::example_station(&limits());
    let s = interpret_stored(Ok(Some(version_5(&d, &c))), none(), &limits());
    assert_eq!((&s.document, &s.catalog), (&d, &c));
    assert!(s.save_now, "the catalogue is not stored on its own yet");

    // A catalogue stored on its own wins.
    let s = interpret_stored(
        Ok(Some(version_5(&d, &c))),
        Ok(Some(catalog_to_json(&Catalog::default()))),
        &limits(),
    );
    assert!(s.catalog.entries.is_empty());
    assert!(!s.save_now);
}

#[test]
fn unreadable_data_is_kept_and_explained() {
    let raw = "{ not json".to_string();
    let s = interpret_stored(Ok(Some(raw.clone())), none(), &limits());
    assert_eq!(s.document.name.as_str(), "Station1");
    let [problem] = s.problems.as_slice() else {
        panic!("{:?}", s.problems)
    };
    assert_eq!(problem.item, StoredItem::Document);
    assert_eq!(problem.raw, raw);
    assert!(
        problem.reason.starts_with("The file is not valid JSON"),
        "{}",
        problem.reason
    );
}

#[test]
fn an_unreadable_catalogue_is_its_own_problem() {
    let d = doc(vec![rack("Lab", 12, vec![])]);
    let s = interpret_stored(Ok(Some(to_json(&d))), Ok(Some(to_json(&d))), &limits());
    assert_eq!(s.document, d, "the document still loads");
    assert!(s.catalog.entries.is_empty());
    let [problem] = s.problems.as_slice() else {
        panic!("{:?}", s.problems)
    };
    assert_eq!(problem.item, StoredItem::Catalog);
    assert_eq!(problem.reason, "This is a station file, not a catalogue.");
}

#[test]
fn memory_store_round_trips() {
    let store = MemoryStore::default();
    assert_eq!(pollster::block_on(store.load()), Ok(None));
    pollster::block_on(store.save("{}".into())).unwrap();
    assert_eq!(pollster::block_on(store.load()), Ok(Some("{}".into())));
}

fn new_session() -> RefCell<Session> {
    let (_, c) = rackwright_core::example::example_station(&limits());
    RefCell::new(Session::new(
        Document::new_default(&limits()),
        c,
        limits(),
        VIEWPORT,
    ))
}

#[test]
fn autosave_writes_the_document_and_catalogue_and_reports_failure() {
    let session = new_session();
    let (documents, catalogs) = (MemoryStore::default(), MemoryStore::default());
    pollster::block_on(autosave(&session, &documents, &catalogs));
    assert_eq!(
        documents.data.borrow().as_deref(),
        Some(session.borrow().save_payload().as_str())
    );
    assert_eq!(
        catalogs.data.borrow().as_deref(),
        Some(session.borrow().catalog_payload().as_str())
    );
    assert_eq!(session.borrow().banner(), None);

    catalogs.fail_saves.set(true);
    pollster::block_on(autosave(&session, &documents, &catalogs));
    assert_eq!(session.borrow().banner(), Some(SAVE_FAILED_BANNER));

    catalogs.fail_saves.set(false);
    pollster::block_on(autosave(&session, &documents, &catalogs));
    assert_eq!(
        session.borrow().banner(),
        None,
        "a successful save clears the banner"
    );
}

#[test]
fn autosave_never_overwrites_unreadable_stored_data() {
    let session = new_session();
    let problem = |item| StartupProblem {
        item,
        raw: "x".into(),
        reason: "bad".into(),
    };
    session
        .borrow_mut()
        .add_startup_problem(problem(StoredItem::Catalog));
    let (documents, catalogs) = (MemoryStore::default(), MemoryStore::default());
    *catalogs.data.borrow_mut() = Some("x".into());
    pollster::block_on(autosave(&session, &documents, &catalogs));
    assert_eq!(catalogs.data.borrow().as_deref(), Some("x"));
    assert!(
        documents.data.borrow().is_some(),
        "the document still saves"
    );

    // Dismissing the problem lets the catalogue be saved again.
    session.borrow_mut().dismiss_startup_problem();
    pollster::block_on(autosave(&session, &documents, &catalogs));
    assert_ne!(catalogs.data.borrow().as_deref(), Some("x"));

    // Problems are shown one at a time, in order.
    let mut s = session.borrow_mut();
    s.add_startup_problem(problem(StoredItem::Document));
    s.add_startup_problem(problem(StoredItem::Catalog));
    assert_eq!(
        s.startup_problem().map(|p| p.item),
        Some(StoredItem::Document)
    );
    s.dismiss_startup_problem();
    assert_eq!(
        s.startup_problem().map(|p| p.item),
        Some(StoredItem::Catalog)
    );
    assert!(s.autosave_allowed(StoredItem::Document));
    assert!(!s.autosave_allowed(StoredItem::Catalog));
}

#[test]
fn stored_under_the_current_key_is_loaded_as_is() {
    let kv = MemoryKeyValue::default();
    kv.set(STORAGE_KEY, "new").unwrap();
    kv.set(LEGACY_STORAGE_KEY, "old").unwrap();
    assert_eq!(load_migrating(&kv), Ok(Some("new".into())));
    assert_eq!(kv.get(LEGACY_STORAGE_KEY), Ok(Some("old".into())));
}

#[test]
fn legacy_document_moves_to_the_current_key() {
    let kv = MemoryKeyValue::default();
    kv.set(LEGACY_STORAGE_KEY, "old").unwrap();
    assert_eq!(load_migrating(&kv), Ok(Some("old".into())));
    assert_eq!(kv.get(STORAGE_KEY), Ok(Some("old".into())));
    assert_eq!(kv.get(LEGACY_STORAGE_KEY), Ok(None));
}

#[test]
fn legacy_document_is_kept_when_it_cannot_be_copied() {
    let kv = MemoryKeyValue::default();
    kv.set(LEGACY_STORAGE_KEY, "old").unwrap();
    kv.fail_writes.set(true);
    assert_eq!(load_migrating(&kv), Ok(Some("old".into())));
    assert_eq!(kv.get(LEGACY_STORAGE_KEY), Ok(Some("old".into())));
}

#[test]
fn nothing_under_either_key_loads_nothing() {
    assert_eq!(load_migrating(&MemoryKeyValue::default()), Ok(None));
}
