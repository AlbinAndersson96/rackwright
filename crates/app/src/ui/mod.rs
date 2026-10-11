//! The Leptos user interface (browser only).

mod core;
mod panels;
mod toolbar;
mod tree;
mod viewport;

use glam::Vec2;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use self::core::{Dialog, Signals, install, now_s, signals, update};
use crate::session::Session;
use crate::storage::{DocumentStore, LocalStorageStore, Startup, interpret_stored};

/// Entry point: installs the panic hook and mounts the app.
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Starting,
    Ready,
    NoWebGpu,
}

#[component]
fn App() -> impl IntoView {
    let status = RwSignal::new(Status::Starting);
    let sig = Signals {
        rev: RwSignal::new(0),
        dragging: RwSignal::new(false),
        over_trash: RwSignal::new(false),
        dialog: RwSignal::new(None),
        menu: RwSignal::new(None),
        renaming: RwSignal::new(None),
        focus_name: RwSignal::new(None),
        tree_drag: RwSignal::new(None),
    };
    leptos::task::spawn_local(async move {
        if !rackwright_render::gpu::webgpu_available().await {
            status.set(Status::NoWebGpu);
            return;
        }
        let limits = rackwright_core::limits::Limits::default();
        let stored = LocalStorageStore.load().await;
        let (document, problem) = match interpret_stored(stored, &limits) {
            Startup::Fresh(d) | Startup::Restored(d) => (d, None),
            Startup::Unreadable { fallback, problem } => (fallback, Some(problem)),
        };
        let mut session = Session::new(document, limits, Vec2::new(800.0, 600.0));
        let has_problem = problem.is_some();
        session.set_startup_problem(problem);
        install(session, sig);
        if has_problem {
            sig.dialog.set(Some(Dialog::StartupProblem));
        }
        install_keyboard();
        status.set(Status::Ready);
    });

    move || {
        match status.get() {
        Status::Starting => view! { <div class="splash">"Loading…"</div> }.into_any(),
        Status::NoWebGpu => view! {
            <div class="splash">
                <h1>"WebGPU is required"</h1>
                <p>"Rackwright needs a browser with WebGPU, such as a current Chrome, Edge or Firefox on Windows."</p>
            </div>
        }
            .into_any(),
        Status::Ready => view! {
            <div class="app" on:pointerdown=move |_| signals().menu.set(None)>
                <toolbar::Toolbar />
                <aside class="sidebar">
                    <section class="panel tree-panel"><tree::Tree /></section>
                    <panels::Properties />
                    <panels::NewDeviceForm />
                    <panels::CatalogPanel />
                    <panels::NewPortForm />
                </aside>
                <main class="view"><viewport::Viewport /></main>
                <toolbar::DialogHost />
            </div>
        }
            .into_any(),
    }
    }
}

/// Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z outside text fields; Esc cancels drags and closes menus.
fn install_keyboard() {
    let handle = window_event_listener(leptos::ev::keydown, move |ev| {
        let in_text_field = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|e| matches!(e.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"));
        let key = ev.key().to_lowercase();
        if key == "escape" {
            signals().menu.set(None);
            update(|s| s.escape(now_s()));
            return;
        }
        if in_text_field || !(ev.ctrl_key() || ev.meta_key()) {
            return;
        }
        match (key.as_str(), ev.shift_key()) {
            ("z", false) => {
                ev.prevent_default();
                update(|s| s.undo());
            }
            ("y", _) | ("z", true) => {
                ev.prevent_default();
                update(|s| s.redo());
            }
            _ => {}
        }
    });
    std::mem::forget(handle);
    // Leaving the window mid-gesture must not leave a drag or orbit running.
    let blur = window_event_listener(leptos::ev::blur, |_| viewport::forward_cancel());
    std::mem::forget(blur);
}
