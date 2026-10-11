//! The browser-side state that is not reactive (session, renderer, DOM handles) and the glue
//! that keeps signals, autosave and the frame loop in step with the session.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::prelude::*;
use rackwright_core::edit::ObjectId;
use rackwright_core::ids::RackId;
use rackwright_render::gpu::{FrameStatus, Renderer};
use rackwright_render::text::CanvasTextRasterizer;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::session::Session;
use crate::storage::{LocalStorageStore, autosave};

/// A modal dialog (spec §4.3 "Dialogs").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    ConfirmNew,
    ConfirmExample,
    ConfirmImport { text: String },
    ConfirmDeleteRack { rack: RackId, name: String },
    Error { message: String },
    StartupProblem,
    DeviceLost,
}

/// Where a context menu was opened, which decides what "Rename" does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOrigin {
    Tree,
    View,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub target: ObjectId,
    pub origin: MenuOrigin,
}

/// What is being renamed inline in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameTarget {
    Document,
    Object(ObjectId),
}

/// Reactive state the components read. All `Copy`, so closures can capture them freely.
#[derive(Clone, Copy)]
pub struct Signals {
    /// Mirrors `Session::ui_revision`; components that show session data track it.
    pub rev: RwSignal<u64>,
    pub dragging: RwSignal<bool>,
    pub over_trash: RwSignal<bool>,
    pub dialog: RwSignal<Option<Dialog>>,
    pub menu: RwSignal<Option<Menu>>,
    pub renaming: RwSignal<Option<RenameTarget>>,
    /// Asks the properties panel to focus this object's name field; the field clears it.
    pub focus_name: RwSignal<Option<ObjectId>>,
    pub tree_drag: RwSignal<Option<RackId>>,
}

pub struct Core {
    pub session: Rc<RefCell<Session>>,
    pub renderer: RefCell<Option<Renderer>>,
    pub text: RefCell<Option<CanvasTextRasterizer>>,
    pub canvas: RefCell<Option<web_sys::HtmlCanvasElement>>,
    pub trash: RefCell<Option<web_sys::HtmlElement>>,
    pub sig: Signals,
    frame_scheduled: Cell<bool>,
    saved_revision: Cell<u64>,
}

thread_local! {
    static CORE: RefCell<Option<Rc<Core>>> = const { RefCell::new(None) };
}

pub fn install(session: Session, sig: Signals) {
    let saved = session.revision();
    let core = Core {
        session: Rc::new(RefCell::new(session)),
        renderer: RefCell::new(None),
        text: RefCell::new(CanvasTextRasterizer::new().ok()),
        canvas: RefCell::new(None),
        trash: RefCell::new(None),
        sig,
        frame_scheduled: Cell::new(false),
        saved_revision: Cell::new(saved),
    };
    CORE.with(|c| *c.borrow_mut() = Some(Rc::new(core)));
}

pub fn core() -> Rc<Core> {
    CORE.with(|c| c.borrow().clone().expect("core installed at startup"))
}

pub fn signals() -> Signals {
    core().sig
}

/// Reads the session.
pub fn read<R>(f: impl FnOnce(&Session) -> R) -> R {
    let core = core();
    let session = core.session.borrow();
    f(&session)
}

/// Changes the session, then syncs signals, autosaves if the document changed, and redraws.
pub fn update<R>(f: impl FnOnce(&mut Session) -> R) -> R {
    let core = core();
    let result = f(&mut core.session.borrow_mut());
    core.after_change();
    result
}

pub fn now_s() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0)
}

impl Core {
    fn after_change(&self) {
        let (ui_revision, dragging, revision, allowed) = {
            let s = self.session.borrow();
            (
                s.ui_revision(),
                s.is_dragging(),
                s.revision(),
                s.autosave_allowed(),
            )
        };
        if self.sig.rev.get_untracked() != ui_revision {
            self.sig.rev.set(ui_revision);
        }
        if self.sig.dragging.get_untracked() != dragging {
            self.sig.dragging.set(dragging);
        }
        if allowed && self.saved_revision.get() != revision {
            self.saved_revision.set(revision);
            save_now();
        }
        request_frame();
    }
}

/// Writes the document to local storage now (and updates the banner).
pub fn save_now() {
    let session = core().session.clone();
    leptos::task::spawn_local(async move {
        autosave(&session, &LocalStorageStore).await;
        let core = core();
        let rev = core.session.borrow().ui_revision();
        core.sig.rev.set(rev);
    });
}

/// Schedules one animation frame (no-op if one is already scheduled).
pub fn request_frame() {
    let core = core();
    if core.frame_scheduled.replace(true) {
        return;
    }
    let callback = Closure::once_into_js(move |_time: f64| frame());
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

/// Draws one frame and keeps the loop running only while something animates.
fn frame() {
    let core = core();
    core.frame_scheduled.set(false);
    let animating = core.session.borrow_mut().advance(now_s());
    let (scene, camera) = {
        let s = core.session.borrow();
        (s.scene(), *s.camera())
    };
    let status = {
        let mut renderer = core.renderer.borrow_mut();
        let mut text = core.text.borrow_mut();
        let (Some(renderer), Some(text)) = (renderer.as_mut(), text.as_mut()) else {
            return;
        };
        if renderer.is_device_lost() {
            core.sig.dialog.set(Some(Dialog::DeviceLost));
            return;
        }
        renderer.render(&scene, &camera, text)
    };
    match status {
        Ok(FrameStatus::Drawn) if !animating => {}
        Ok(_) => request_frame(),
        Err(e) => leptos::logging::error!("render failed: {e}"),
    }
}
