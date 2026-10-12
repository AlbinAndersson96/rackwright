//! Persistence (spec §5): swappable stores for the document's and the catalogue's JSON, and
//! startup recovery.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;

use rackwright_core::edit::merge_catalog;
use rackwright_core::example::example_station;
use rackwright_core::file_format::{catalog_from_json, load_station};
use rackwright_core::limits::Limits;
use rackwright_core::model::{Catalog, Document};

use crate::session::{StartupProblem, StoredItem};

/// Browser local-storage key of the autosaved document.
pub const STORAGE_KEY: &str = "rackwright.document";

/// Browser local-storage key of the autosaved catalogue.
pub const CATALOG_STORAGE_KEY: &str = "rackwright.catalog";

/// Key the document was autosaved under before the app was renamed to Rackwright.
pub const LEGACY_STORAGE_KEY: &str = "teststationview.document";

pub const SAVE_FAILED_BANNER: &str =
    "Changes can't be saved in this browser — use Export to keep your work.";

/// Where the current document, or the catalogue, is kept between sessions (one store each).
/// Async so a server-backed store can implement it later without changing callers.
pub trait DocumentStore {
    /// The stored JSON, if any.
    fn load(&self) -> impl Future<Output = Result<Option<String>, String>>;
    fn save(&self, json: String) -> impl Future<Output = Result<(), String>>;
}

/// What to start with, given what the stores returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    pub document: Document,
    pub catalog: Catalog,
    /// Stored data that exists but cannot be loaded. The app starts from the default document
    /// or an empty catalogue, shows each problem, and does not overwrite that stored data
    /// until the user decides.
    pub problems: Vec<StartupProblem>,
    /// The catalogue was taken from the stored document (format 5 or older), so it is not
    /// stored on its own yet: save now.
    pub save_now: bool,
}

/// With no stored document (or storage unavailable) this is a first visit, which opens the
/// example station and merges its models into the catalogue.
pub fn interpret_stored(
    document: Result<Option<String>, String>,
    catalog: Result<Option<String>, String>,
    limits: &Limits,
) -> Startup {
    let mut problems = Vec::new();
    let mut problem = |item, raw, reason: String| {
        problems.push(StartupProblem { item, raw, reason });
    };
    let (stored_document, legacy) = match document {
        Ok(Some(raw)) => match load_station(&raw, limits) {
            Ok(loaded) => (Some(loaded.document), loaded.legacy_catalog),
            Err(e) => {
                problem(StoredItem::Document, raw, e.to_string());
                (Some(Document::new_default(limits)), None)
            }
        },
        Ok(None) | Err(_) => (None, None),
    };
    let mut save_now = false;
    let catalog = match catalog {
        Ok(Some(raw)) => catalog_from_json(&raw, limits).unwrap_or_else(|e| {
            problem(StoredItem::Catalog, raw, e.to_string());
            Catalog::default()
        }),
        Ok(None) | Err(_) => legacy.inspect(|_| save_now = true).unwrap_or_default(),
    };
    let (catalog, document) = match stored_document {
        Some(document) => (catalog, document),
        None => {
            let (example, models) = example_station(limits);
            merge_catalog(&catalog, &models, &example)
        }
    };
    Startup {
        document,
        catalog,
        problems,
        save_now,
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

/// One key of the browser's `localStorage`.
#[cfg(target_arch = "wasm32")]
pub struct LocalStorageStore {
    key: &'static str,
}

#[cfg(target_arch = "wasm32")]
impl LocalStorageStore {
    pub const DOCUMENT: LocalStorageStore = LocalStorageStore { key: STORAGE_KEY };
    pub const CATALOG: LocalStorageStore = LocalStorageStore {
        key: CATALOG_STORAGE_KEY,
    };

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
        let result = Self::storage().and_then(|s| {
            if self.key == STORAGE_KEY {
                load_migrating(&s)
            } else {
                KeyValue::get(&s, self.key)
            }
        });
        std::future::ready(result)
    }

    fn save(&self, json: String) -> impl Future<Output = Result<(), String>> {
        let result = Self::storage().and_then(|s| KeyValue::set(&s, self.key, &json));
        std::future::ready(result)
    }
}

/// Saves the session's document and catalogue, each where autosave is allowed, and updates
/// the banner.
pub async fn autosave(
    session: &RefCell<crate::session::Session>,
    documents: &impl DocumentStore,
    catalogs: &impl DocumentStore,
) {
    let (document, catalog) = {
        let s = session.borrow();
        (
            s.autosave_allowed(StoredItem::Document)
                .then(|| s.save_payload()),
            s.autosave_allowed(StoredItem::Catalog)
                .then(|| s.catalog_payload()),
        )
    };
    if document.is_none() && catalog.is_none() {
        return;
    }
    let mut ok = true;
    if let Some(json) = document {
        ok &= documents.save(json).await.is_ok();
    }
    if let Some(json) = catalog {
        ok &= catalogs.save(json).await.is_ok();
    }
    let banner = (!ok).then(|| SAVE_FAILED_BANNER.to_string());
    session.borrow_mut().set_banner(banner);
}
