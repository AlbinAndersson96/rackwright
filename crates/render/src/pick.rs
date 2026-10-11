//! CPU picking and drag targeting (spec §4.2 "Picking").

use glam::{Vec2, Vec3};
use rackwright_core::edit::ObjectId;
use rackwright_core::ids::{DeviceId, RackId};
use rackwright_core::limits::Limits;
use rackwright_core::model::{Device, Document};
use rackwright_core::port_grid::{Cell, PushDir, push_direction};

use crate::layout::{
    Aabb, CABLE_RADIUS_MM, PLINTH_MM, RACK_WIDTH_MM, U_MM, cable_points, device_box, device_face,
    port_cell_rect, port_marker_box, rack_left_x, rack_parts,
};

/// A ray this close to a cable's axis hits it, so thin cables are easy to click.
pub const CABLE_PICK_RADIUS_MM: f32 = 2.0 * CABLE_RADIUS_MM;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    /// Unit length.
    pub dir: Vec3,
}

impl Ray {
    /// Distance along the ray to the box's entry point (0 if the origin is inside).
    pub fn hit_aabb(&self, b: &Aabb) -> Option<f32> {
        let mut t_near = 0.0f32;
        let mut t_far = f32::INFINITY;
        for axis in 0..3 {
            let (o, d) = (self.origin[axis], self.dir[axis]);
            let (lo, hi) = (b.min[axis], b.max[axis]);
            if d == 0.0 {
                // Parallel to this slab: inside it or never (avoids 0 × ∞ = NaN on edges).
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let (t1, t2) = ((lo - o) / d, (hi - o) / d);
            t_near = t_near.max(t1.min(t2));
            t_far = t_far.min(t1.max(t2));
        }
        (t_far >= t_near).then_some(t_near)
    }

    /// The shortest distance between the ray and the segment `a`–`b`, and the distance along
    /// the ray to that closest approach. `None` if the closest approach lies behind the origin.
    pub fn closest_to_segment(&self, a: Vec3, b: Vec3) -> Option<(f32, f32)> {
        let perpendicular = |v: Vec3| v - self.dir * v.dot(self.dir);
        let w = a - self.origin;
        let d = b - a;
        let (w_perp, d_perp) = (perpendicular(w), perpendicular(d));
        let len2 = d_perp.length_squared();
        let u = if len2 > 1e-9 {
            (-w_perp.dot(d_perp) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let t = (w + d * u).dot(self.dir);
        (t >= 0.0).then(|| ((w_perp + d_perp * u).length(), t))
    }

    /// Point where the ray crosses the plane of constant `z`, if it does (in front of the origin).
    pub fn hit_plane_z(&self, z: f32) -> Option<Vec3> {
        if self.dir.z.abs() < 1e-6 {
            return None;
        }
        let t = (z - self.origin.z) / self.dir.z;
        (t >= 0.0).then(|| self.origin + self.dir * t)
    }
}

/// The nearest rack, device, port or cable under the ray. A port beats a cable in front of it,
/// so connected ports stay easy to pick.
pub fn pick(doc: &Document, limits: &Limits, ray: &Ray) -> Option<ObjectId> {
    pick_hit(doc, limits, ray).map(|(id, _)| id)
}

/// Like `pick`, plus the world point where the ray enters the object.
pub fn pick_hit(doc: &Document, limits: &Limits, ray: &Ray) -> Option<(ObjectId, Vec3)> {
    let mut best: Option<(f32, ObjectId)> = None;
    let consider = |best: &mut Option<(f32, ObjectId)>, t: Option<f32>, id: ObjectId| {
        if let Some(t) = t
            && best.is_none_or(|(bt, _)| t < bt)
        {
            *best = Some((t, id));
        }
    };
    for (ri, rack) in doc.racks.iter().enumerate() {
        for part in rack_parts(ri, rack) {
            consider(&mut best, ray.hit_aabb(&part), ObjectId::Rack(rack.id));
        }
        for device in &rack.devices {
            consider(
                &mut best,
                ray.hit_aabb(&device_box(ri, device)),
                ObjectId::Device(device.id),
            );
            for port in &device.ports {
                let cell = Cell {
                    row: port.row,
                    col: port.col,
                };
                consider(
                    &mut best,
                    ray.hit_aabb(&port_marker_box(
                        ri,
                        device,
                        cell,
                        port.kind,
                        port.gender,
                        limits,
                    )),
                    ObjectId::Port(port.id),
                );
            }
        }
    }
    if !matches!(best, Some((_, ObjectId::Port(_)))) {
        for cable in &doc.cables {
            let Some(points) = cable_points(doc, limits, cable) else {
                continue;
            };
            let t = points
                .windows(2)
                .filter_map(|s| ray.closest_to_segment(s[0], s[1]))
                .filter(|&(distance, _)| distance <= CABLE_PICK_RADIUS_MM)
                .map(|(_, t)| t)
                .reduce(f32::min);
            consider(&mut best, t, ObjectId::Cable(cable.id));
        }
    }
    best.map(|(t, id)| (id, ray.origin + ray.dir * t))
}

/// Which U of `device` lies under world height `y` (0 = its bottom U), clamped to the device.
pub fn grab_offset_u(device: &Device, y: f32) -> u32 {
    let u_under = ((y - PLINTH_MM) / U_MM).floor() as i64 + 1;
    (u_under - i64::from(device.bottom_u)).clamp(0, i64::from(device.height_u) - 1) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceTarget {
    pub rack: RackId,
    /// Snapped, unclamped bottom U (core clamps it into the rack). Never below 1.
    pub bottom_u: u32,
}

/// Where a device dragged by its `grab_offset_u`-th U would land: the rack whose outline the
/// ray crosses on the front plane, and the U under the pointer minus the grab offset.
pub fn device_drop_target(doc: &Document, ray: &Ray, grab_offset_u: u32) -> Option<DeviceTarget> {
    let p = ray.hit_plane_z(0.0)?;
    let rack_index = (0..doc.racks.len()).find(|&i| {
        let left = rack_left_x(i);
        p.x >= left && p.x <= left + RACK_WIDTH_MM
    })?;
    let u_under = ((p.y - PLINTH_MM) / U_MM).floor() as i64 + 1;
    let bottom = (u_under - i64::from(grab_offset_u)).clamp(1, i64::from(u32::MAX)) as u32;
    Some(DeviceTarget {
        rack: doc.racks[rack_index].id,
        bottom_u: bottom,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortTarget {
    pub cell: Cell,
    pub dir: PushDir,
}

/// The cell of `device`'s face under the ray, and the push direction from the pointer's offset
/// within that cell. `None` when the ray misses the face.
pub fn port_drop_target(
    doc: &Document,
    limits: &Limits,
    device_id: DeviceId,
    ray: &Ray,
) -> Option<PortTarget> {
    let (ri, device) = doc.racks.iter().enumerate().find_map(|(ri, r)| {
        r.devices
            .iter()
            .find(|d| d.id == device_id)
            .map(|d| (ri, d))
    })?;
    let face = device_face(ri, device);
    let p = ray.hit_plane_z(0.0)?;
    let p2 = Vec2::new(p.x, p.y);
    if !face.contains(p2) {
        return None;
    }
    let rows = device.height_u * limits.port_rows_per_u;
    let cell_size = port_cell_rect(ri, device, Cell { row: 0, col: 0 }, limits).size();
    let local = p2 - face.min;
    let col = ((local.x / cell_size.x).floor() as u32).min(limits.port_cols - 1);
    let row = ((local.y / cell_size.y).floor() as u32).min(rows - 1);
    let cell = Cell { row, col };
    let centre = port_cell_rect(ri, device, cell, limits).center();
    let dx = (p.x - centre.x) / cell_size.x;
    let dy = (p.y - centre.y) / cell_size.y;
    Some(PortTarget {
        cell,
        dir: push_direction(dx, dy),
    })
}
