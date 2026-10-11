mod common;

use std::cell::RefCell;

use common::*;
use rackwright_app::session::{Session, StartupProblem};
use rackwright_app::storage::*;
use rackwright_core::file_format::to_json;
use rackwright_core::model::Document;

#[test]
fn nothing_stored_starts_with_the_example() {
    let s = interpret_stored(Ok(None), &limits());
    let Startup::Fresh(d) = s else {
        panic!("{s:?}")
    };
    assert_eq!(d.name.as_str(), "Example Station");
    assert!(!d.cables.is_empty() && !d.catalog.is_empty());
}

#[test]
fn unavailable_storage_starts_fresh() {
    assert!(matches!(
        interpret_stored(Err("blocked".into()), &limits()),
        Startup::Fresh(_)
    ));
}

#[test]
fn stored_document_is_restored() {
    let d = doc(vec![rack("Lab", 12, vec![device("DMM", 1, 1)])]);
    assert_eq!(
        interpret_stored(Ok(Some(to_json(&d))), &limits()),
        Startup::Restored(d)
    );
}

#[test]
fn unreadable_data_is_kept_and_explained() {
    let raw = "{ not json".to_string();
    let Startup::Unreadable { fallback, problem } =
        interpret_stored(Ok(Some(raw.clone())), &limits())
    else {
        panic!("expected Unreadable")
    };
    assert_eq!(fallback.name.as_str(), "Station1");
    assert_eq!(problem.raw, raw);
    assert!(
        problem.reason.starts_with("The file is not valid JSON"),
        "{}",
        problem.reason
    );
}

#[test]
fn memory_store_round_trips() {
    let store = MemoryStore::default();
    assert_eq!(pollster::block_on(store.load()), Ok(None));
    pollster::block_on(store.save("{}".into())).unwrap();
    assert_eq!(pollster::block_on(store.load()), Ok(Some("{}".into())));
}

#[test]
fn autosave_writes_the_document_and_reports_failure() {
    let session = RefCell::new(Session::new(
        Document::new_default(&limits()),
        limits(),
        VIEWPORT,
    ));
    let store = MemoryStore::default();
    pollster::block_on(autosave(&session, &store));
    assert_eq!(
        store.data.borrow().as_deref(),
        Some(session.borrow().save_payload().as_str())
    );
    assert_eq!(session.borrow().banner(), None);

    store.fail_saves.set(true);
    pollster::block_on(autosave(&session, &store));
    assert_eq!(session.borrow().banner(), Some(SAVE_FAILED_BANNER));

    store.fail_saves.set(false);
    pollster::block_on(autosave(&session, &store));
    assert_eq!(
        session.borrow().banner(),
        None,
        "a successful save clears the banner"
    );
}

#[test]
fn autosave_never_overwrites_unreadable_stored_data() {
    let session = RefCell::new(Session::new(
        Document::new_default(&limits()),
        limits(),
        VIEWPORT,
    ));
    session
        .borrow_mut()
        .set_startup_problem(Some(StartupProblem {
            raw: "x".into(),
            reason: "bad".into(),
        }));
    let store = MemoryStore::default();
    *store.data.borrow_mut() = Some("x".into());
    pollster::block_on(autosave(&session, &store));
    assert_eq!(store.data.borrow().as_deref(), Some("x"));
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
