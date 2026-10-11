//! World-space geometry of racks, devices and ports (spec §4.2 "World units").
//!
//! Coordinates are millimetres, right-handed, Y up. Racks stand in a row along +X, their front
//! plane is z = 0 and they extend towards -Z. U1 sits directly on top of the plinth.

use glam::{Vec2, Vec3};
use rackwright_core::edit::ObjectId;
use rackwright_core::ids::PortId;
use rackwright_core::limits::Limits;
use rackwright_core::model::{Cable, Device, Document, Gender, PortKind, Rack};
use rackwright_core::port_grid::Cell;

pub const U_MM: f32 = 44.45;
pub const FRONT_WIDTH_MM: f32 = 482.6;
pub const RACK_WIDTH_MM: f32 = 600.0;
pub const POST_WIDTH_MM: f32 = (RACK_WIDTH_MM - FRONT_WIDTH_MM) / 2.0;
pub const RACK_DEPTH_MM: f32 = 800.0;
pub const DEVICE_DEPTH_MM: f32 = 450.0;
pub const RACK_GAP_MM: f32 = 40.0;
pub const PLINTH_MM: f32 = 60.0;
pub const HEADER_MM: f32 = 100.0;
pub const PANEL_MM: f32 = 20.0;
pub const PORT_PROTRUSION_MM: f32 = 6.0;
/// The floor grid reaches this far beyond the racks on every side.
pub const FLOOR_MARGIN_MM: f32 = 1500.0;
/// Labels float this far in front of the surface they are printed on.
pub const LABEL_OFFSET_MM: f32 = 1.0;
/// Cables are tubes of this radius, drawn as this many straight segments.
pub const CABLE_RADIUS_MM: f32 = 3.5;
pub const CABLE_SEGMENTS: usize = 24;

/// Axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Aabb {
        Aabb { min, max }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn union(&self, other: &Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn translated(&self, offset: Vec3) -> Aabb {
        Aabb {
            min: self.min + offset,
            max: self.max + offset,
        }
    }
}

/// A rectangle in a plane of constant z, facing +Z (where labels and port grids live).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceRect {
    pub min: Vec2,
    pub max: Vec2,
    pub z: f32,
}

impl FaceRect {
    pub fn center(&self) -> Vec3 {
        let c = (self.min + self.max) * 0.5;
        Vec3::new(c.x, c.y, self.z)
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn translated(&self, offset: Vec3) -> FaceRect {
        FaceRect {
            min: self.min + offset.truncate(),
            max: self.max + offset.truncate(),
            z: self.z + offset.z,
        }
    }
}

pub fn rack_left_x(index: usize) -> f32 {
    index as f32 * (RACK_WIDTH_MM + RACK_GAP_MM)
}

/// Bottom edge of rack unit `u` (U1 is the lowest).
pub fn u_bottom_y(u: u32) -> f32 {
    PLINTH_MM + (u.saturating_sub(1)) as f32 * U_MM
}

pub fn rack_total_height(rack: &Rack) -> f32 {
    PLINTH_MM + rack.height_u as f32 * U_MM + HEADER_MM
}

/// Frame parts of a rack: two side posts, plinth, header panel, top cover.
pub fn rack_parts(index: usize, rack: &Rack) -> Vec<Aabb> {
    let left = rack_left_x(index);
    let right = left + RACK_WIDTH_MM;
    let inner_left = left + POST_WIDTH_MM;
    let inner_right = right - POST_WIDTH_MM;
    let total = rack_total_height(rack);
    let units_top = u_bottom_y(rack.height_u + 1);
    vec![
        Aabb::new(
            Vec3::new(left, 0.0, -RACK_DEPTH_MM),
            Vec3::new(inner_left, total, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_right, 0.0, -RACK_DEPTH_MM),
            Vec3::new(right, total, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_left, 0.0, -RACK_DEPTH_MM),
            Vec3::new(inner_right, PLINTH_MM, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_left, units_top, -PANEL_MM),
            Vec3::new(inner_right, total, 0.0),
        ),
        Aabb::new(
            Vec3::new(inner_left, total - PANEL_MM, -RACK_DEPTH_MM),
            Vec3::new(inner_right, total, 0.0),
        ),
    ]
}

/// The header panel's front, where the rack name is printed.
pub fn rack_name_rect(index: usize, rack: &Rack) -> FaceRect {
    let inner_left = rack_left_x(index) + POST_WIDTH_MM;
    let units_top = u_bottom_y(rack.height_u + 1);
    FaceRect {
        min: Vec2::new(inner_left + 10.0, units_top + 15.0),
        max: Vec2::new(
            inner_left + FRONT_WIDTH_MM - 10.0,
            units_top + HEADER_MM - 15.0,
        ),
        z: LABEL_OFFSET_MM,
    }
}

/// U number label on the front of the left post.
pub fn u_label_rect(index: usize, u: u32) -> FaceRect {
    let left = rack_left_x(index);
    let bottom = u_bottom_y(u);
    FaceRect {
        min: Vec2::new(left + 8.0, bottom + U_MM * 0.2),
        max: Vec2::new(left + POST_WIDTH_MM - 8.0, bottom + U_MM * 0.8),
        z: LABEL_OFFSET_MM,
    }
}

pub fn device_box(rack_index: usize, device: &Device) -> Aabb {
    units_box(rack_index, device.bottom_u, device.height_u)
}

/// The box a device of `height_u` units would occupy with its bottom at `bottom_u`
/// (used for drag ghosts, which may not correspond to a device in the document).
pub fn units_box(rack_index: usize, bottom_u: u32, height_u: u32) -> Aabb {
    let x = rack_left_x(rack_index) + POST_WIDTH_MM;
    let y = u_bottom_y(bottom_u);
    Aabb::new(
        Vec3::new(x, y, -DEVICE_DEPTH_MM),
        Vec3::new(x + FRONT_WIDTH_MM, y + height_u as f32 * U_MM, 0.0),
    )
}

/// The device's front face (z = 0).
pub fn device_face(rack_index: usize, device: &Device) -> FaceRect {
    let x = rack_left_x(rack_index) + POST_WIDTH_MM;
    let y = u_bottom_y(device.bottom_u);
    FaceRect {
        min: Vec2::new(x, y),
        max: Vec2::new(x + FRONT_WIDTH_MM, y + device.height_u as f32 * U_MM),
        z: 0.0,
    }
}

/// Where the device name is printed: centred on the face, at most one U tall.
pub fn device_name_rect(rack_index: usize, device: &Device) -> FaceRect {
    let face = device_face(rack_index, device);
    let height = U_MM * 0.6;
    let cy = (face.min.y + face.max.y) * 0.5;
    FaceRect {
        min: Vec2::new(face.min.x + 10.0, cy - height * 0.5),
        max: Vec2::new(face.max.x - 10.0, cy + height * 0.5),
        z: LABEL_OFFSET_MM,
    }
}

/// One cell of the device's port grid (row 0 at the bottom).
pub fn port_cell_rect(rack_index: usize, device: &Device, cell: Cell, limits: &Limits) -> FaceRect {
    let face = device_face(rack_index, device);
    let rows = (device.height_u * limits.port_rows_per_u).max(1) as f32;
    let w = FRONT_WIDTH_MM / limits.port_cols.max(1) as f32;
    let h = face.size().y / rows;
    let min = Vec2::new(
        face.min.x + cell.col as f32 * w,
        face.min.y + cell.row as f32 * h,
    );
    FaceRect {
        min,
        max: min + Vec2::new(w, h),
        z: 0.0,
    }
}

/// The outline of a port's marker, in units of the plain square marker's side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerShape {
    Square,
    /// A cylinder sticking out of the face, with this diameter.
    Round(f32),
    /// Width × height.
    Rect(f32, f32),
}

pub fn marker_shape(kind: PortKind) -> MarkerShape {
    match kind {
        PortKind::Unspecified | PortKind::Other => MarkerShape::Square,
        PortKind::Bnc => MarkerShape::Round(1.0),
        PortKind::Sma => MarkerShape::Round(0.6),
        PortKind::NType => MarkerShape::Round(1.2),
        PortKind::Banana => MarkerShape::Round(0.5),
        PortKind::Usb => MarkerShape::Rect(1.2, 0.5),
        PortKind::Lan => MarkerShape::Rect(1.0, 0.8),
        PortKind::Gpib => MarkerShape::Rect(1.9, 0.5),
        PortKind::DSub => MarkerShape::Rect(1.6, 0.6),
        PortKind::Power => MarkerShape::Rect(1.2, 0.9),
    }
}

/// How far a marker sticks out, in units of `PORT_PROTRUSION_MM`: male connectors stand out
/// like pins, female ones sit nearly flush like sockets.
pub fn marker_depth(gender: Gender) -> f32 {
    match gender {
        Gender::Unspecified | Gender::Other => 1.0,
        Gender::Male => 1.6,
        Gender::Female => 0.5,
    }
}

/// The marker drawn for a port of `kind` and `gender`, centred in the upper part of its cell.
/// Its outline follows `marker_shape` and its depth `marker_depth`; a round marker's box is the
/// cylinder's bounds.
pub fn port_marker_box(
    rack_index: usize,
    device: &Device,
    cell: Cell,
    kind: PortKind,
    gender: Gender,
    limits: &Limits,
) -> Aabb {
    let r = port_cell_rect(rack_index, device, cell, limits);
    let size = r.size();
    let side = size.x.min(size.y) * 0.45;
    let (w, h) = match marker_shape(kind) {
        MarkerShape::Square => (1.0, 1.0),
        MarkerShape::Round(d) => (d, d),
        MarkerShape::Rect(w, h) => (w, h),
    };
    let half = Vec2::new(side * w, side * h) * 0.5;
    let cx = (r.min.x + r.max.x) * 0.5;
    let cy = r.min.y + size.y * 0.62;
    Aabb::new(
        Vec3::new(cx - half.x, cy - half.y, 0.0),
        Vec3::new(
            cx + half.x,
            cy + half.y,
            PORT_PROTRUSION_MM * marker_depth(gender),
        ),
    )
}

/// Where a port's name appears (under its marker) while hovered or selected.
pub fn port_label_rect(
    rack_index: usize,
    device: &Device,
    cell: Cell,
    limits: &Limits,
) -> FaceRect {
    let r = port_cell_rect(rack_index, device, cell, limits);
    let size = r.size();
    FaceRect {
        min: Vec2::new(r.min.x + size.x * 0.05, r.min.y + size.y * 0.06),
        max: Vec2::new(r.max.x - size.x * 0.05, r.min.y + size.y * 0.34),
        z: LABEL_OFFSET_MM,
    }
}

/// Where a cable plugs into the port drawn as `marker`: the centre of its front face.
pub fn marker_anchor(marker: &Aabb) -> Vec3 {
    let c = marker.center();
    Vec3::new(c.x, c.y, marker.max.z)
}

/// Where a cable plugs into `port`, if it exists.
pub fn port_anchor(doc: &Document, limits: &Limits, port: PortId) -> Option<Vec3> {
    object_bounds(doc, limits, ObjectId::Port(port)).map(|m| marker_anchor(&m))
}

/// `CABLE_SEGMENTS + 1` points of a cable from `a` to `b`: a cubic Bézier curve whose inner
/// control points stand out in front of the ends (+Z) and lower, so the cable leaves both ports
/// towards the viewer and sags between them.
pub fn cable_path(a: Vec3, b: Vec3) -> Vec<Vec3> {
    let span = a.distance(b);
    let out = (span * 0.35).clamp(60.0, 300.0);
    let lift = Vec3::new(0.0, -span * 0.2, out);
    let (p1, p2) = (a + lift, b + lift);
    (0..=CABLE_SEGMENTS)
        .map(|i| {
            let t = i as f32 / CABLE_SEGMENTS as f32;
            let s = 1.0 - t;
            a * (s * s * s) + p1 * (3.0 * s * s * t) + p2 * (3.0 * s * t * t) + b * (t * t * t)
        })
        .collect()
}

/// The resting path of `cable`; `None` if an end is not a port of `doc`.
pub fn cable_points(doc: &Document, limits: &Limits, cable: &Cable) -> Option<Vec<Vec3>> {
    let a = port_anchor(doc, limits, cable.a)?;
    let b = port_anchor(doc, limits, cable.b)?;
    Some(cable_path(a, b))
}

/// Bounds of everything in the document; a default 42U rack's bounds when it has no racks.
pub fn scene_bounds(doc: &Document) -> Aabb {
    let default_rack;
    let racks: Vec<(usize, &Rack)> = if doc.racks.is_empty() {
        default_rack = Rack {
            id: rackwright_core::ids::RackId::new(),
            name: rackwright_core::name::Name::parse("R", &Limits::default()).expect("valid"),
            height_u: rackwright_core::model::DEFAULT_RACK_HEIGHT_U,
            devices: Vec::new(),
        };
        vec![(0, &default_rack)]
    } else {
        doc.racks.iter().enumerate().collect()
    };
    racks
        .into_iter()
        .flat_map(|(i, r)| rack_parts(i, r))
        .reduce(|a, b| a.union(&b))
        .expect("at least one rack part")
}

/// The world-space bounds of a rack (all its parts), device, port marker or cable.
pub fn object_bounds(doc: &Document, limits: &Limits, id: ObjectId) -> Option<Aabb> {
    match id {
        ObjectId::Rack(rack_id) => {
            let (ri, rack) = doc
                .racks
                .iter()
                .enumerate()
                .find(|(_, r)| r.id == rack_id)?;
            rack_parts(ri, rack).into_iter().reduce(|a, b| a.union(&b))
        }
        ObjectId::Device(device_id) => doc.racks.iter().enumerate().find_map(|(ri, r)| {
            r.devices
                .iter()
                .find(|d| d.id == device_id)
                .map(|d| device_box(ri, d))
        }),
        ObjectId::Port(port_id) => doc.racks.iter().enumerate().find_map(|(ri, r)| {
            r.devices.iter().find_map(|d| {
                d.ports.iter().find(|p| p.id == port_id).map(|p| {
                    port_marker_box(
                        ri,
                        d,
                        Cell {
                            row: p.row,
                            col: p.col,
                        },
                        p.kind,
                        p.gender,
                        limits,
                    )
                })
            })
        }),
        ObjectId::Cable(cable_id) => {
            let cable = doc.cable(cable_id)?;
            let points = cable_points(doc, limits, cable)?;
            let r = Vec3::splat(CABLE_RADIUS_MM);
            points
                .iter()
                .map(|&p| Aabb::new(p - r, p + r))
                .reduce(|a, b| a.union(&b))
        }
    }
}
