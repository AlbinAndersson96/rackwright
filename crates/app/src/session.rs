//! Everything the app knows, independent of the DOM: document and history, selection, view,
//! drag preview and status (spec §3.6, §4.3, §5). The UI calls these methods and redraws.

use glam::{Vec2, Vec3};
use rackwright_core::edit::{
    ObjectId, Plan, Rejection, plan_add_rack, plan_move_rack, plan_new_document, plan_remove_cable,
    plan_remove_device, plan_remove_model, plan_remove_port, plan_remove_rack, plan_rename_cable,
    plan_rename_device, plan_rename_document, plan_rename_model, plan_rename_port,
    plan_rename_rack, plan_save_model, plan_set_cable_color, plan_set_device_color,
    plan_set_device_height, plan_set_port_gender, plan_set_port_kind, plan_set_rack_height,
    plan_update_model,
};
use rackwright_core::editor::Editor;
use rackwright_core::file_format::{from_json, to_json};
use rackwright_core::ids::{CableId, DeviceId, ModelId, PortId, RackId};
use rackwright_core::limits::Limits;
use rackwright_core::model::{Document, Gender, PortKind, Rgb};
use rackwright_render::camera::OrbitCamera;
use rackwright_render::layout::{object_bounds, scene_bounds};
use rackwright_render::pick::Ray;
use rackwright_render::scene::{
    CablePreview, Ghost, Scene, SceneInput, animation_targets, build_scene,
};
use rackwright_render::view::ViewState;

use crate::forms::{
    capitalise, export_file_name, parse_document_name, parse_height, parse_model_text, parse_name,
};
use crate::interaction::Mode;

/// What `request_delete` decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted,
    /// A rack that still holds devices: ask first, then call `delete`.
    NeedsConfirmation,
}

/// Stored data that could not be loaded at startup (spec §5 "Startup").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupProblem {
    pub raw: String,
    pub reason: String,
}

pub struct Session {
    pub limits: Limits,
    editor: Editor,
    selection: Option<ObjectId>,
    pub(crate) view: ViewState,
    pub(crate) viewport: Vec2,
    pub(crate) mode: Mode,
    pub(crate) hovered_port: Option<PortId>,
    pub(crate) preview: Option<Plan>,
    pub(crate) ghost: Option<Ghost>,
    pub(crate) cable_preview: Option<CablePreview>,
    pub(crate) hidden_cable: Option<CableId>,
    last_frame_s: Option<f64>,
    revision: u64,
    ui_revision: u64,
    banner: Option<String>,
    startup_problem: Option<StartupProblem>,
}

impl Session {
    /// `viewport` is the canvas size in CSS pixels.
    pub fn new(document: Document, limits: Limits, viewport: Vec2) -> Session {
        let viewport = viewport.max(Vec2::ONE);
        let camera = OrbitCamera::front_view(&scene_bounds(&document), viewport.x / viewport.y);
        let mut view = ViewState::new(camera);
        view.motion.snap(&animation_targets(&document, &limits));
        Session {
            limits,
            editor: Editor::new(document),
            selection: None,
            view,
            viewport,
            mode: Mode::Idle,
            hovered_port: None,
            preview: None,
            ghost: None,
            cable_preview: None,
            hidden_cable: None,
            last_frame_s: None,
            revision: 0,
            ui_revision: 0,
            banner: None,
            startup_problem: None,
        }
    }

    /// The committed document (what the tree, properties and autosave show).
    pub fn document(&self) -> &Document {
        self.editor.document()
    }

    /// What the 3D view shows: a drag preview's document, or the committed one.
    pub fn shown_document(&self) -> &Document {
        self.preview
            .as_ref()
            .map_or(self.editor.document(), |p| &p.document)
    }

    pub fn selection(&self) -> Option<ObjectId> {
        self.selection
    }

    /// Changes whenever the committed document changes (drives autosave).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Changes whenever anything the DOM shows changes (document, selection, history, banner,
    /// dialogs). Pointer hover, drag previews and the camera do not count.
    pub fn ui_revision(&self) -> u64 {
        self.ui_revision
    }

    pub fn camera(&self) -> &OrbitCamera {
        self.view.camera()
    }

    pub fn viewport(&self) -> Vec2 {
        self.viewport
    }

    pub fn set_viewport(&mut self, viewport: Vec2) {
        self.viewport = viewport.max(Vec2::ONE);
    }

    pub(crate) fn aspect(&self) -> f32 {
        self.viewport.x / self.viewport.y
    }

    /// The pick ray through a canvas position in CSS pixels.
    pub fn ray_at(&self, pos: Vec2) -> Ray {
        let ndc = rackwright_render::camera::pixel_to_ndc(pos, self.viewport);
        self.view.camera().ray(ndc, self.aspect())
    }

    /// Canvas position (CSS pixels) of a world point.
    pub fn world_to_pixel(&self, p: Vec3) -> Vec2 {
        let ndc = self
            .view
            .camera()
            .view_proj(self.aspect())
            .project_point3(p);
        Vec2::new(
            (ndc.x + 1.0) * 0.5 * self.viewport.x,
            (1.0 - ndc.y) * 0.5 * self.viewport.y,
        )
    }

    pub fn select(&mut self, selection: Option<ObjectId>) {
        if self.selection != selection {
            self.selection = selection;
            self.ui_revision += 1;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    /// Undo; ignored while a drag is in progress.
    pub fn undo(&mut self) {
        if self.mode.is_idle() && self.editor.undo() {
            self.document_changed();
        }
    }

    /// Redo; ignored while a drag is in progress.
    pub fn redo(&mut self) {
        if self.mode.is_idle() && self.editor.redo() {
            self.document_changed();
        }
    }

    /// Commits a successful plan and selects what it created or moved.
    pub(crate) fn commit(&mut self, plan: Plan) {
        let subject = plan.subject;
        self.editor.commit(plan);
        if let Some(subject) = subject {
            self.selection = Some(subject);
        }
        self.document_changed();
    }

    /// Commits `result`, or returns the rejection as a sentence for the UI.
    pub(crate) fn apply(&mut self, result: Result<Plan, Rejection>) -> Result<(), String> {
        let plan = result.map_err(|r| capitalise(&r.to_string()))?;
        self.commit(plan);
        Ok(())
    }

    fn document_changed(&mut self) {
        if let Some(sel) = self.selection
            && !exists(self.editor.document(), sel)
        {
            self.selection = None;
        }
        if let Some(port) = self.hovered_port
            && !exists(self.editor.document(), ObjectId::Port(port))
        {
            self.hovered_port = None;
        }
        self.revision += 1;
        self.ui_revision += 1;
    }

    pub fn add_rack(&mut self) {
        let plan = plan_add_rack(self.editor.document(), &self.limits);
        self.commit(plan);
    }

    pub fn move_rack(&mut self, rack: RackId, new_index: usize) {
        let result = plan_move_rack(self.editor.document(), rack, new_index);
        let _ = self.apply(result);
    }

    /// Renames a rack, device, port or cable from user text.
    pub fn rename(&mut self, id: ObjectId, text: &str) -> Result<(), String> {
        let name = parse_name(text, &self.limits)?;
        let doc = self.editor.document();
        let result = match id {
            ObjectId::Rack(r) => plan_rename_rack(doc, r, name),
            ObjectId::Device(d) => plan_rename_device(doc, d, name),
            ObjectId::Port(p) => plan_rename_port(doc, p, name),
            ObjectId::Cable(c) => plan_rename_cable(doc, c, name),
        };
        self.apply(result)
    }

    pub fn rename_document(&mut self, text: &str) -> Result<(), String> {
        let name = parse_document_name(text)?;
        let plan = plan_rename_document(self.editor.document(), name);
        self.commit(plan);
        Ok(())
    }

    pub fn set_rack_height(&mut self, rack: RackId, text: &str) -> Result<(), String> {
        let height = parse_height(text)?;
        let result = plan_set_rack_height(self.editor.document(), &self.limits, rack, height);
        self.apply(result)
    }

    pub fn set_device_height(&mut self, device: DeviceId, text: &str) -> Result<(), String> {
        let height = parse_height(text)?;
        let result = plan_set_device_height(self.editor.document(), &self.limits, device, height);
        self.apply(result)
    }

    pub fn set_device_color(&mut self, device: DeviceId, color: Rgb) {
        let result = plan_set_device_color(self.editor.document(), device, color);
        let _ = self.apply(result);
    }

    /// "Save as model": a catalogue entry from the device, which becomes linked to it.
    pub fn save_model(
        &mut self,
        device: DeviceId,
        manufacturer: &str,
        model: &str,
    ) -> Result<(), String> {
        let manufacturer = parse_model_text(manufacturer)?;
        let model = parse_model_text(model)?;
        let result = plan_save_model(self.editor.document(), device, manufacturer, model);
        self.apply(result)
    }

    /// "Update model from this device".
    pub fn update_model(&mut self, device: DeviceId) -> Result<(), String> {
        let result = plan_update_model(self.editor.document(), device);
        self.apply(result)
    }

    pub fn rename_model(
        &mut self,
        id: ModelId,
        manufacturer: &str,
        model: &str,
    ) -> Result<(), String> {
        let manufacturer = parse_model_text(manufacturer)?;
        let model = parse_model_text(model)?;
        let result = plan_rename_model(self.editor.document(), id, manufacturer, model);
        self.apply(result)
    }

    pub fn remove_model(&mut self, id: ModelId) {
        let result = plan_remove_model(self.editor.document(), id);
        let _ = self.apply(result);
    }

    pub fn set_port_kind(&mut self, port: PortId, kind: PortKind) {
        let result = plan_set_port_kind(self.editor.document(), port, kind);
        let _ = self.apply(result);
    }

    pub fn set_port_gender(&mut self, port: PortId, gender: Gender) {
        let result = plan_set_port_gender(self.editor.document(), port, gender);
        let _ = self.apply(result);
    }

    pub fn set_cable_color(&mut self, cable: CableId, color: Rgb) {
        let result = plan_set_cable_color(self.editor.document(), cable, color);
        let _ = self.apply(result);
    }

    /// Deletes `id`, unless it is a rack with devices, which needs confirmation first.
    pub fn request_delete(&mut self, id: ObjectId) -> DeleteOutcome {
        if let ObjectId::Rack(r) = id
            && self
                .editor
                .document()
                .rack(r)
                .is_some_and(|rack| !rack.devices.is_empty())
        {
            return DeleteOutcome::NeedsConfirmation;
        }
        self.delete(id);
        DeleteOutcome::Deleted
    }

    /// Deletes `id` without asking.
    pub fn delete(&mut self, id: ObjectId) {
        let doc = self.editor.document();
        let result = match id {
            ObjectId::Rack(r) => plan_remove_rack(doc, r),
            ObjectId::Device(d) => plan_remove_device(doc, d),
            ObjectId::Port(p) => plan_remove_port(doc, p),
            ObjectId::Cable(c) => plan_remove_cable(doc, c),
        };
        let _ = self.apply(result);
    }

    /// Replaces the document with the default one (undoable); called after confirmation.
    pub fn new_document(&mut self, now_s: f64) {
        let plan = plan_new_document(&self.limits);
        self.selection = None;
        self.commit(plan);
        self.reset_view(now_s);
    }

    /// Replaces the document with the example station (undoable); called after confirmation.
    pub fn load_example(&mut self, now_s: f64) {
        let plan = Plan {
            document: rackwright_core::example::example_document(&self.limits),
            subject: None,
        };
        self.selection = None;
        self.commit(plan);
        self.view
            .motion
            .snap(&animation_targets(self.editor.document(), &self.limits));
        self.reset_view(now_s);
    }

    /// Replaces the document with an imported file; clears history (spec §5 "Import").
    pub fn import(&mut self, text: &str, now_s: f64) -> Result<(), String> {
        let document = from_json(text, &self.limits).map_err(|e| e.to_string())?;
        self.editor.replace_clearing_history(document);
        self.selection = None;
        self.view
            .motion
            .snap(&animation_targets(self.editor.document(), &self.limits));
        self.document_changed();
        self.reset_view(now_s);
        Ok(())
    }

    /// `(file name, JSON)` for Export.
    pub fn export(&self) -> (String, String) {
        let doc = self.editor.document();
        (export_file_name(doc), to_json(doc))
    }

    /// What autosave writes.
    pub fn save_payload(&self) -> String {
        to_json(self.editor.document())
    }

    pub fn banner(&self) -> Option<&str> {
        self.banner.as_deref()
    }

    pub fn set_banner(&mut self, banner: Option<String>) {
        if self.banner != banner {
            self.banner = banner;
            self.ui_revision += 1;
        }
    }

    pub fn startup_problem(&self) -> Option<&StartupProblem> {
        self.startup_problem.as_ref()
    }

    /// While a startup problem is open, autosave must not overwrite the stored data.
    pub fn autosave_allowed(&self) -> bool {
        self.startup_problem.is_none()
    }

    pub fn set_startup_problem(&mut self, problem: Option<StartupProblem>) {
        self.startup_problem = problem;
        self.ui_revision += 1;
    }

    /// Tweens the camera to fit `id` (tree double-click).
    pub fn frame(&mut self, id: ObjectId, now_s: f64) {
        if let Some(bounds) = object_bounds(self.editor.document(), &self.limits, id) {
            let c = *self.view.camera();
            let to = OrbitCamera::framing(&bounds, self.aspect(), c.yaw, c.pitch);
            self.view.animate_to(to, now_s);
        }
    }

    /// Tweens to the default front view of all racks.
    pub fn reset_view(&mut self, now_s: f64) {
        let to = OrbitCamera::front_view(&scene_bounds(self.editor.document()), self.aspect());
        self.view.animate_to(to, now_s);
    }

    /// The device the new-port form targets: the selected device, or the selected port's device.
    pub fn port_form_device(&self) -> Option<DeviceId> {
        match self.selection? {
            ObjectId::Device(d) => Some(d),
            ObjectId::Port(p) => self.editor.document().port(p).map(|(_, d, _)| d.id),
            ObjectId::Rack(_) | ObjectId::Cable(_) => None,
        }
    }

    /// Advances animations to `now_s`. Returns `true` while another frame is needed.
    pub fn advance(&mut self, now_s: f64) -> bool {
        let dt = self
            .last_frame_s
            .map_or(0.0, |last| (now_s - last).max(0.0) as f32);
        self.last_frame_s = Some(now_s);
        let doc = self
            .preview
            .as_ref()
            .map_or(self.editor.document(), |p| &p.document);
        let animating = self.view.advance(doc, &self.limits, now_s, dt);
        if !animating {
            self.last_frame_s = None;
        }
        animating
    }

    /// The draw lists for the current frame.
    pub fn scene(&self) -> Scene {
        let input = SceneInput {
            document: self.shown_document(),
            limits: &self.limits,
            selection: self.selection,
            hovered_port: self.hovered_port,
            ghost: self.ghost,
            cable_preview: self.cable_preview,
            hidden_cable: self.hidden_cable,
        };
        build_scene(&input, &self.view.motion)
    }
}

fn exists(doc: &Document, id: ObjectId) -> bool {
    match id {
        ObjectId::Rack(r) => doc.rack(r).is_some(),
        ObjectId::Device(d) => doc.device(d).is_some(),
        ObjectId::Port(p) => doc.port(p).is_some(),
        ObjectId::Cable(c) => doc.cable(c).is_some(),
    }
}
