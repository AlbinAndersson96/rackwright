//! Persistence (spec §5): a swappable store of the document's JSON, and startup recovery.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;

use rackwright_core::example::example_document;
use rackwright_core::file_format::from_json;
use rackwright_core::limits::Limits;
use rackwright_core::model::Document;

use crate::session::StartupProblem;

/// Browser local-storage key of the autosaved document.
pub const STORAGE_KEY: &str = "rackwright.document";

/// Key the document was autosaved under before the app was renamed to Rackwright.
pub const LEGACY_STORAGE_KEY: &str = "teststationview.document";

pub const SAVE_FAILED_BANNER: &str =
    "Changes can't be saved in this browser — use Export to keep your work.";

/// Where the current document is kept between sessions. Async so a server-backed store can
/// implement it later without changing callers.
pub trait DocumentStore {
    /// The stored document's JSON, if any.
    fn load(&self) -> impl Future<Output = Result<Option<String>, String>>;
    fn save(&self, json: String) -> impl Future<Output = Result<(), String>>;
}

/// What to start with, given what the store returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    /// Nothing stored (or storage unavailable): a first visit, which opens the example station.
    Fresh(Document),
    Restored(Document),
    /// Stored data exists but cannot be loaded. Start from the default document, show the
    /// problem, and do not overwrite the stored data until the user decides.
    Unreadable {
        fallback: Document,
        problem: StartupProblem,
    },
}

pub fn interpret_stored(stored: Result<Option<String>, String>, limits: &Limits) -> Startup {
    match stored {
        Ok(Some(raw)) => match from_json(&raw, limits) {
            Ok(doc) => Startup::Restored(doc),
            Err(e) => Startup::Unreadable {
                fallback: Document::new_default(limits),
                problem: StartupProblem {
                    raw,
                    reason: e.to_string(),
                },
            },
        },
        Ok(None) | Err(_) => Startup::Fresh(example_document(limits)),
    }
}

/// In-memory store for tests.
#[derive(Debug, Default)]
pub struct MemoryStore {
    pub data: RefCell<Option<String>>,
    /// When set, `save` fails like a full or disabled browser storage.
    pub fail_saves: Cell<bool>,
}

impl DocumentStore for MemoryStore {
    fn load(&self) -> impl Future<Output = Result<Option<String>, String>> {
        std::future::ready(Ok(self.data.borrow().clone()))
    }

    fn save(&self, json: String) -> impl Future<Output = Result<(), String>> {
        let result = if self.fail_saves.get() {
            Err("storage unavailable".to_string())
        } else {
            *self.data.borrow_mut() = Some(json);
            Ok(())
        };
        std::future::ready(result)
    }
}

/// String key-value storage like `localStorage`, so the key migration is testable natively.
pub trait KeyValue {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn remove(&self, key: &str) -> Result<(), String>;
}

/// The stored document, moving one saved under `LEGACY_STORAGE_KEY` to `STORAGE_KEY`. The
/// legacy entry is removed only once the copy has been written, so a failure loses nothing.
pub fn load_migrating(kv: &impl KeyValue) -> Result<Option<String>, String> {
    if let Some(current) = kv.get(STORAGE_KEY)? {
        return Ok(Some(current));
    }
    let Some(legacy) = kv.get(LEGACY_STORAGE_KEY)? else {
        return Ok(None);
    };
    if kv.set(STORAGE_KEY, &legacy).is_ok() {
        let _ = kv.remove(LEGACY_STORAGE_KEY);
    }
    Ok(Some(legacy))
}

/// In-memory key-value storage for tests.
#[derive(Debug, Default)]
pub struct MemoryKeyValue {
    pub entries: RefCell<HashMap<String, String>>,
    /// When set, `set` and `remove` fail like a full or read-only browser storage.
    pub fail_writes: Cell<bool>,
}

impl KeyValue for MemoryKeyValue {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.entries.borrow().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        if self.fail_writes.get() {
            return Err("storage unavailable".to_string());
        }
        self.entries
            .borrow_mut()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), String> {
        if self.fail_writes.get() {
            return Err("storage unavailable".to_string());
        }
        self.entries.borrow_mut().remove(key);
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl KeyValue for web_sys::Storage {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        self.get_item(key)
            .map_err(|_| "could not read local storage".to_string())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.set_item(key, value)
            .map_err(|_| "could not write local storage".to_string())
    }

    fn remove(&self, key: &str) -> Result<(), String> {
        self.remove_item(key)
            .map_err(|_| "could not write local storage".to_string())
    }
}

/// The browser's `localStorage`.
#[cfg(target_arch = "wasm32")]
pub struct LocalStorageStore;

#[cfg(target_arch = "wasm32")]
impl LocalStorageStore {
    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or("no window")?
            .local_storage()
            .map_err(|_| "local storage is blocked".to_string())?
            .ok_or_else(|| "local storage is unavailable".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
impl DocumentStore for LocalStorageStore {
    fn load(&self) -> impl Future<Output = Result<Option<String>, String>> {
        let result = Self::storage().and_then(|s| load_migrating(&s));
        std::future::ready(result)
    }

    fn save(&self, json: String) -> impl Future<Output = Result<(), String>> {
        let result = Self::storage().and_then(|s| KeyValue::set(&s, STORAGE_KEY, &json));
        std::future::ready(result)
    }
}

/// Saves the session's document if autosave is allowed, and updates the banner.
pub async fn autosave(session: &RefCell<crate::session::Session>, store: &impl DocumentStore) {
    let payload = {
        let s = session.borrow();
        if !s.autosave_allowed() {
            return;
        }
        s.save_payload()
    };
    let result = store.save(payload).await;
    let banner = result.err().map(|_| SAVE_FAILED_BANNER.to_string());
    session.borrow_mut().set_banner(banner);
}
