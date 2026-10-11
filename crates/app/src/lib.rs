//! Rackwright web app: document session, interaction and persistence (pure, testable
//! natively) plus the Leptos UI (browser only).

pub mod forms;
pub mod interaction;
pub mod session;
pub mod storage;
#[cfg(target_arch = "wasm32")]
pub mod ui;
