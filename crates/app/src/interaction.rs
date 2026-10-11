//! Pointer and keyboard handling for the 3D view (spec §4.3 "Interaction controller").
//!
//! Positions are canvas-relative CSS pixels. `over_trash` says whether the pointer is over the
//! trash zone (the UI knows where that element is).

use glam::{Vec2, Vec3};
use rackwright_core::edit::{
    DeviceSource, ObjectId, Plan, PortSource, plan_connect, plan_device_drop, plan_port_drop,
    plan_remove_cable, plan_remove_device, plan_remove_port, plan_replug,
};
use rackwright_core::ids::{CableId, DeviceId, ModelId, PortId};
use rackwright_core::model::{Gender, PortKind, Rgb};
use rackwright_core::name::Name;
use rackwright_core::port_grid::Cell;
use rackwright_render::layout::{
    device_face, object_bounds, port_anchor, port_marker_box, units_box,
};
use rackwright_render::pick::{
    device_drop_target, grab_offset_u, pick, pick_hit, port_drop_target,
};
use rackwright_render::scene::{CABLE_WARNING, CablePreview, Ghost, contrast_text, port_color};

use crate::session::Session;

/// Pointer movement (CSS px) that turns a press into a drag.
pub const DRAG_THRESHOLD_PX: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

/// What is being dragged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragSource {
    NewDevice {
        name: Name,
        height_u: u32,
    },
    Device(DeviceId),
    /// A new device from a catalogue entry (the Catalogue panel's handle).
    Model(ModelId),
    NewPort {
        device: DeviceId,
        name: Name,
        kind: PortKind,
        gender: Gender,
    },
    Port {
        device: DeviceId,
        port: PortId,
    },
    /// A new cable from port `from` (Shift-drag from a free port, or the port panel's handle).
    Cable {
        from: PortId,
    },
    /// The end of `cable` plugged into `end`, being re-plugged (Shift-drag from a connected
    /// port, or the port panel's handle).
    CableEnd {
        cable: CableId,
        end: PortId,
    },
}

impl DragSource {
    fn is_port(&self) -> bool {
        matches!(self, DragSource::NewPort { .. } | DragSource::Port { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mode {
    Idle,
    /// Left button down, not yet moved far enough to be a drag.
    Pressed {
        start: Vec2,
        hit: Option<(ObjectId, Vec3)>,
        /// Shift was held: dragging from a port draws a cable instead of moving the port.
        connect: bool,
    },
    Orbiting {
        last: Vec2,
    },
    Panning {
        last: Vec2,
    },
    Dragging {
        source: DragSource,
        grab_offset_u: u32,
    },
}

impl Mode {
    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Mode::Idle)
    }
}

impl Session {
    /// `true` while a device or port is being dragged (the UI shows the trash zone).
    /// Drawing a cable does not count: there is nothing to delete.
    pub fn is_dragging(&self) -> bool {
        matches!(&self.mode, Mode::Dragging { source, .. } if !matches!(source, DragSource::Cable { .. }))
    }

    /// The loose cable being drawn, when the pointer is not over a port it can connect to.
    pub fn cable_preview(&self) -> Option<CablePreview> {
        self.cable_preview
    }

    /// Whether `pos` lies on the canvas. Off-canvas positions never target anything.
    fn on_canvas(&self, pos: Vec2) -> bool {
        pos.x >= 0.0 && pos.y >= 0.0 && pos.x <= self.viewport.x && pos.y <= self.viewport.y
    }

    /// `true` while any gesture (press, orbit, pan, drag) is in progress.
    pub fn is_busy(&self) -> bool {
        !self.mode.is_idle()
    }

    pub fn pointer_down(&mut self, pos: Vec2, button: Button) {
        self.pointer_down_with_shift(pos, button, false);
    }

    /// Like `pointer_down`; with `shift`, a drag from a port draws a cable.
    pub fn pointer_down_with_shift(&mut self, pos: Vec2, button: Button, shift: bool) {
        if !self.mode.is_idle() {
            return;
        }
        self.mode = match button {
            Button::Left => {
                let hit = pick_hit(self.document(), &self.limits, &self.ray_at(pos));
                Mode::Pressed {
                    start: pos,
                    hit,
                    connect: shift,
                }
            }
            Button::Middle => Mode::Panning { last: pos },
            Button::Right => Mode::Idle,
        };
    }

    pub fn pointer_move(&mut self, pos: Vec2, over_trash: bool, now_s: f64) {
        match self.mode.clone() {
            Mode::Idle => {
                let target = if self.on_canvas(pos) {
                    pick(self.document(), &self.limits, &self.ray_at(pos))
                } else {
                    None
                };
                let hovered = match target {
                    Some(ObjectId::Port(p)) => Some(p),
                    _ => None,
                };
                self.hovered_port = hovered;
            }
            Mode::Pressed {
                start,
                hit,
                connect,
            } => {
                if pos.distance(start) < DRAG_THRESHOLD_PX {
                    return;
                }
                match hit {
                    Some((ObjectId::Port(p), _)) if connect => {
                        let source = match self.document().cable_at_port(p) {
                            Some(c) => DragSource::CableEnd {
                                cable: c.id,
                                end: p,
                            },
                            None => DragSource::Cable { from: p },
                        };
                        self.start_drag(source, now_s);
                    }
                    Some((ObjectId::Device(d), at)) => {
                        let grab = self
                            .document()
                            .device(d)
                            .map_or(0, |(_, device)| grab_offset_u(device, at.y));
                        self.mode = Mode::Dragging {
                            source: DragSource::Device(d),
                            grab_offset_u: grab,
                        };
                    }
                    Some((ObjectId::Port(p), _)) => {
                        let Some((_, device, _)) = self.document().port(p) else {
                            return;
                        };
                        let device = device.id;
                        self.start_drag(DragSource::Port { device, port: p }, now_s);
                    }
                    _ => {
                        self.mode = Mode::Orbiting { last: start };
                    }
                }
                self.pointer_move(pos, over_trash, now_s);
            }
            Mode::Orbiting { last } => {
                let d = pos - last;
                self.view.camera_mut().orbit(d.x, d.y);
                self.mode = Mode::Orbiting { last: pos };
            }
            Mode::Panning { last } => {
                let d = pos - last;
                let height = self.viewport.y;
                self.view.camera_mut().pan(d.x, d.y, height);
                self.mode = Mode::Panning { last: pos };
            }
            Mode::Dragging {
                source,
                grab_offset_u,
            } => {
                self.update_drag(&source, grab_offset_u, pos, over_trash);
            }
        }
    }

    pub fn pointer_up(&mut self, pos: Vec2, over_trash: bool, now_s: f64) {
        match std::mem::replace(&mut self.mode, Mode::Idle) {
            Mode::Pressed { hit, .. } => self.select(hit.map(|(id, _)| id)),
            Mode::Dragging {
                source,
                grab_offset_u,
            } => {
                self.update_drag(&source, grab_offset_u, pos, over_trash);
                if over_trash {
                    self.drop_in_trash(&source);
                } else if let Some(plan) = self.preview.take() {
                    self.commit(plan);
                }
                self.end_drag(&source, now_s);
            }
            _ => {}
        }
    }

    /// Esc: cancels a drag without changing anything.
    pub fn escape(&mut self, now_s: f64) {
        if let Mode::Dragging { source, .. } = std::mem::replace(&mut self.mode, Mode::Idle) {
            self.end_drag(&source, now_s);
        }
    }

    /// The browser cancelled the pointer or took away its capture: end any gesture, change
    /// nothing.
    pub fn pointer_cancel(&mut self, now_s: f64) {
        if let Mode::Dragging { source, .. } = std::mem::replace(&mut self.mode, Mode::Idle) {
            self.end_drag(&source, now_s);
        }
    }

    /// Mouse wheel over the view (positive = zoom out).
    pub fn wheel(&mut self, delta: f32) {
        self.view.camera_mut().zoom(delta);
    }

    /// A drag that starts on a sidebar handle ("new device" / "new port").
    pub fn start_drag(&mut self, source: DragSource, now_s: f64) {
        if source.is_port() {
            let device = match &source {
                DragSource::NewPort { device, .. } | DragSource::Port { device, .. } => *device,
                _ => unreachable!(),
            };
            let doc = self.document();
            let face = doc.racks.iter().enumerate().find_map(|(ri, r)| {
                r.devices
                    .iter()
                    .find(|d| d.id == device)
                    .map(|d| device_face(ri, d))
            });
            if let Some(face) = face {
                let aspect = self.aspect();
                self.view.enter_port_mode(&face, aspect, now_s);
            }
        }
        self.preview = None;
        self.ghost = None;
        self.cable_preview = None;
        self.hidden_cable = None;
        self.mode = Mode::Dragging {
            source,
            grab_offset_u: 0,
        };
    }

    /// Double-click in the 3D view: selects the object under `pos` and frames it.
    /// Empty space (or a position off the canvas) does nothing.
    pub fn double_click(&mut self, pos: Vec2, now_s: f64) {
        if !self.mode.is_idle() || !self.on_canvas(pos) {
            return;
        }
        if let Some(id) = pick(self.document(), &self.limits, &self.ray_at(pos)) {
            self.select(Some(id));
            self.frame(id, now_s);
        }
    }

    /// The object a right-click at `pos` refers to (for the context menu).
    pub fn context_target(&self, pos: Vec2) -> Option<ObjectId> {
        if !self.mode.is_idle() {
            return None;
        }
        pick(self.document(), &self.limits, &self.ray_at(pos))
    }

    pub fn hovered_port(&self) -> Option<PortId> {
        self.hovered_port
    }

    pub fn ghost(&self) -> Option<Ghost> {
        self.ghost
    }

    fn end_drag(&mut self, source: &DragSource, now_s: f64) {
        self.preview = None;
        self.ghost = None;
        self.cable_preview = None;
        self.hidden_cable = None;
        if source.is_port() {
            self.view.exit_port_mode(now_s);
        }
    }

    fn drop_in_trash(&mut self, source: &DragSource) {
        let doc = self.document();
        let result = match source {
            DragSource::Device(d) => plan_remove_device(doc, *d),
            DragSource::Port { port, .. } => plan_remove_port(doc, *port),
            DragSource::CableEnd { cable, .. } => plan_remove_cable(doc, *cable),
            DragSource::NewDevice { .. }
            | DragSource::Model(_)
            | DragSource::NewPort { .. }
            | DragSource::Cable { .. } => return,
        };
        let _ = self.apply(result);
    }

    /// Recomputes the preview document and the ghost for the pointer at `pos`.
    fn update_drag(
        &mut self,
        source: &DragSource,
        grab_offset_u: u32,
        pos: Vec2,
        over_trash: bool,
    ) {
        self.preview = None;
        self.ghost = None;
        self.cable_preview = None;
        self.hidden_cable = None;
        if over_trash || !self.on_canvas(pos) {
            return;
        }
        if matches!(
            source,
            DragSource::Cable { .. } | DragSource::CableEnd { .. }
        ) {
            self.update_cable_drag(source, pos);
            return;
        }
        let ray = self.ray_at(pos);
        let doc = self.document();
        let limits = &self.limits;
        match source {
            DragSource::NewDevice { .. } | DragSource::Device(_) | DragSource::Model(_) => {
                let (core_source, height_u, color) = match source {
                    DragSource::NewDevice { name, height_u } => (
                        DeviceSource::New {
                            name: name.clone(),
                            height_u: *height_u,
                        },
                        *height_u,
                        Rgb::NEUTRAL_GREY,
                    ),
                    DragSource::Device(d) => {
                        let Some((_, device)) = doc.device(*d) else {
                            return;
                        };
                        (DeviceSource::Existing(*d), device.height_u, device.color)
                    }
                    DragSource::Model(m) => {
                        let Some(entry) = doc.model(*m) else {
                            return;
                        };
                        (DeviceSource::Model(*m), entry.height_u, entry.color)
                    }
                    _ => unreachable!(),
                };
                let Some(target) = device_drop_target(doc, &ray, grab_offset_u) else {
                    return;
                };
                match plan_device_drop(doc, limits, &core_source, target.rack, target.bottom_u) {
                    Ok(plan) => {
                        let aabb = plan
                            .subject
                            .and_then(|s| object_bounds(&plan.document, limits, s));
                        self.ghost = aabb.map(|aabb| Ghost {
                            aabb,
                            color,
                            valid: true,
                        });
                        self.preview = Some(plan);
                    }
                    Err(_) => {
                        let Some(ri) = doc.racks.iter().position(|r| r.id == target.rack) else {
                            return;
                        };
                        let rack_height = doc.racks[ri].height_u;
                        let bottom = target
                            .bottom_u
                            .min(rack_height.saturating_sub(height_u) + 1)
                            .max(1);
                        let aabb = units_box(ri, bottom, height_u);
                        self.ghost = Some(Ghost {
                            aabb,
                            color,
                            valid: false,
                        });
                    }
                }
            }
            DragSource::Cable { .. } | DragSource::CableEnd { .. } => {
                unreachable!("handled above")
            }
            DragSource::NewPort { device, .. } | DragSource::Port { device, .. } => {
                let (core_source, kind, gender) = match source {
                    DragSource::NewPort {
                        name, kind, gender, ..
                    } => (
                        PortSource::New {
                            name: name.clone(),
                            kind: *kind,
                            gender: *gender,
                        },
                        *kind,
                        *gender,
                    ),
                    DragSource::Port { port, .. } => {
                        let Some((_, _, p)) = doc.port(*port) else {
                            return;
                        };
                        (PortSource::Existing(*port), p.kind, p.gender)
                    }
                    _ => unreachable!(),
                };
                let Some((ri, d)) = doc.racks.iter().enumerate().find_map(|(ri, r)| {
                    r.devices.iter().find(|d| d.id == *device).map(|d| (ri, d))
                }) else {
                    return;
                };
                let ink = contrast_text(d.color);
                let color = port_color(kind).unwrap_or(Rgb {
                    r: ink[0],
                    g: ink[1],
                    b: ink[2],
                });
                let Some(target) = port_drop_target(doc, limits, *device, &ray) else {
                    return;
                };
                match plan_port_drop(doc, limits, *device, &core_source, target.cell, target.dir) {
                    Ok(plan) => {
                        let aabb = plan
                            .subject
                            .and_then(|s| object_bounds(&plan.document, limits, s));
                        self.ghost = aabb.map(|aabb| Ghost {
                            aabb,
                            color,
                            valid: true,
                        });
                        self.preview = Some(plan);
                    }
                    Err(_) => {
                        let cell = Cell {
                            row: target.cell.row,
                            col: target.cell.col,
                        };
                        let aabb = port_marker_box(ri, d, cell, kind, gender, limits);
                        self.ghost = Some(Ghost {
                            aabb,
                            color,
                            valid: false,
                        });
                    }
                }
            }
        }
    }
}

impl Session {
    /// The cable being drawn (`Cable`) or re-plugged (`CableEnd`). Over a port it can be plugged
    /// into, previews the planned document; over a port that cannot take it, a red cable to that
    /// port; anywhere else, a loose cable to the pointer. A re-plugged cable is hidden while its
    /// loose end is drawn as the preview.
    fn update_cable_drag(&mut self, source: &DragSource, pos: Vec2) {
        let ray = self.ray_at(pos);
        let doc = self.document();
        let limits = &self.limits;
        // The port the moving end leaves from, and the cable being re-plugged, if any.
        let (fixed, moving, color) = match *source {
            DragSource::Cable { from } => (from, None, Rgb::CABLE_BLUE),
            DragSource::CableEnd { cable, end } => {
                let Some(c) = doc.cable(cable) else { return };
                let fixed = if c.a == end { c.b } else { c.a };
                (fixed, Some((cable, end)), c.color)
            }
            _ => unreachable!("only cable drags"),
        };
        let Some(start) = port_anchor(doc, limits, fixed) else {
            return;
        };
        let hit = pick_hit(doc, limits, &ray);
        let target = match hit {
            Some((ObjectId::Port(p), _)) => Some(p),
            _ => None,
        };
        enum Outcome {
            Plug(Plan),
            /// Back on the port the end came from: show the cable as it is.
            Unchanged,
            /// Where the drawn cable ends, and whether releasing there would be accepted.
            Loose(Option<(Vec3, bool)>),
        }
        let planned = |p: PortId| match moving {
            None => plan_connect(doc, limits, fixed, p),
            Some((cable, end)) => plan_replug(doc, cable, end, p),
        };
        let outcome = match hit {
            Some((ObjectId::Port(p), _)) if moving.is_some_and(|(_, end)| end == p) => {
                Outcome::Unchanged
            }
            Some((ObjectId::Port(p), _)) if p != fixed || moving.is_some() => match planned(p) {
                Ok(plan) => Outcome::Plug(plan),
                Err(_) => Outcome::Loose(port_anchor(doc, limits, p).map(|end| (end, false))),
            },
            Some((_, point)) => Outcome::Loose(Some((point, true))),
            None => Outcome::Loose(ray.hit_plane_z(0.0).map(|point| (point, true))),
        };
        self.hovered_port = target;
        match outcome {
            Outcome::Plug(plan) => {
                // A cable between different connector types is allowed but drawn in amber.
                let mismatched = match plan.subject {
                    Some(ObjectId::Cable(id)) => plan
                        .document
                        .cable(id)
                        .filter(|c| plan.document.cable_mismatch(c).is_some())
                        .map(|_| id),
                    _ => None,
                };
                if let (Some(id), Some(p)) = (mismatched, target) {
                    self.hidden_cable = Some(id);
                    self.cable_preview =
                        port_anchor(&plan.document, limits, p).map(|to| CablePreview {
                            from: start,
                            to,
                            color: CABLE_WARNING,
                            valid: true,
                        });
                }
                self.preview = Some(plan);
            }
            Outcome::Unchanged => {}
            Outcome::Loose(end) => {
                self.hidden_cable = moving.map(|(cable, _)| cable);
                self.cable_preview = end.map(|(to, valid)| CablePreview {
                    from: start,
                    to,
                    color,
                    valid,
                });
            }
        }
    }
}
