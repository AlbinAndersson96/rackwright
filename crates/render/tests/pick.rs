mod common;

use rackwright_core::model::{Gender, PortKind};

use common::*;
use glam::Vec3;
use rackwright_core::edit::ObjectId;
use rackwright_core::port_grid::{Cell, PushDir};
use rackwright_render::layout::*;
use rackwright_render::pick::*;

/// A ray straight into the scene (towards -Z) through world point (x, y).
fn ray_at(x: f32, y: f32) -> Ray {
    Ray {
        origin: Vec3::new(x, y, 5000.0),
        dir: Vec3::NEG_Z,
    }
}

#[test]
fn ray_box_intersection() {
    let b = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(ray_at(0.0, 0.0).hit_aabb(&b), Some(4999.0));
    assert_eq!(ray_at(5.0, 0.0).hit_aabb(&b), None);
    let behind = Ray {
        origin: Vec3::new(0.0, 0.0, -10.0),
        dir: Vec3::NEG_Z,
    };
    assert_eq!(behind.hit_aabb(&b), None);
}

#[test]
fn ray_plane_intersection() {
    assert_eq!(
        ray_at(3.0, 4.0).hit_plane_z(0.0),
        Some(Vec3::new(3.0, 4.0, 0.0))
    );
    let parallel = Ray {
        origin: Vec3::ZERO,
        dir: Vec3::X,
    };
    assert_eq!(parallel.hit_plane_z(0.0), None);
}

#[test]
fn picks_the_nearest_object() {
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let d = with_ports(device("D", 5, 1), vec![p]);
    let did = d.id;
    let r = rack("R", 42, vec![d]);
    let rid = r.id;
    let doc = doc(vec![r]);
    let dev = &doc.racks[0].devices[0];

    let marker = port_marker_box(
        0,
        dev,
        Cell { row: 0, col: 0 },
        PortKind::Unspecified,
        Gender::Unspecified,
        &limits(),
    )
    .center();
    assert_eq!(
        pick(&doc, &limits(), &ray_at(marker.x, marker.y)),
        Some(ObjectId::Port(pid))
    );
    let face = device_face(0, dev).center();
    assert_eq!(
        pick(&doc, &limits(), &ray_at(face.x + 150.0, face.y)),
        Some(ObjectId::Device(did))
    );
    assert_eq!(
        pick(&doc, &limits(), &ray_at(10.0, 500.0)),
        Some(ObjectId::Rack(rid))
    );
    assert_eq!(pick(&doc, &limits(), &ray_at(-500.0, 500.0)), None);
}

#[test]
fn grab_offset_is_the_unit_under_the_pointer() {
    let d = device("D", 10, 3);
    assert_eq!(grab_offset_u(&d, u_bottom_y(10) + 1.0), 0);
    assert_eq!(grab_offset_u(&d, u_bottom_y(12) + 1.0), 2);
    assert_eq!(
        grab_offset_u(&d, u_bottom_y(30)),
        2,
        "clamped to the device"
    );
    assert_eq!(grab_offset_u(&d, 0.0), 0, "clamped to the device");
}

#[test]
fn device_drop_target_snaps_to_units_and_racks() {
    let doc = doc(vec![rack("A", 42, vec![]), rack("B", 42, vec![])]);
    let x_b = rack_left_x(1) + 300.0;
    let t = device_drop_target(&doc, &ray_at(x_b, u_bottom_y(12) + 5.0), 0).unwrap();
    assert_eq!(
        t,
        DeviceTarget {
            rack: doc.racks[1].id,
            bottom_u: 12
        }
    );
    let t = device_drop_target(&doc, &ray_at(x_b, u_bottom_y(12) + 5.0), 1).unwrap();
    assert_eq!(t.bottom_u, 11, "grabbed one unit above its bottom");
    let t = device_drop_target(&doc, &ray_at(x_b, 1.0), 2).unwrap();
    assert_eq!(t.bottom_u, 1, "never below U1");
    assert_eq!(
        device_drop_target(&doc, &ray_at(rack_left_x(1) - 20.0, 500.0), 0),
        None,
        "in the gap"
    );
}

#[test]
fn port_drop_target_finds_cell_and_push_direction() {
    let d = device("D", 3, 2);
    let did = d.id;
    let doc = doc(vec![rack("R", 42, vec![d])]);
    let dev = &doc.racks[0].devices[0];
    let cell = Cell { row: 1, col: 3 };
    let c = port_cell_rect(0, dev, cell, &limits()).center();
    let w = FRONT_WIDTH_MM / 5.0;

    let left = port_drop_target(&doc, &limits(), did, &ray_at(c.x - w * 0.3, c.y)).unwrap();
    assert_eq!(
        left,
        PortTarget {
            cell,
            dir: PushDir::Right
        }
    );
    let below = port_drop_target(&doc, &limits(), did, &ray_at(c.x, c.y - U_MM * 0.3)).unwrap();
    assert_eq!(
        below,
        PortTarget {
            cell,
            dir: PushDir::Up
        }
    );
    let face = device_face(0, dev);
    assert_eq!(
        port_drop_target(&doc, &limits(), did, &ray_at(face.max.x + 5.0, c.y)),
        None
    );
}

#[test]
fn pointer_exactly_on_a_box_edge_still_picks_cleanly() {
    let b = Aabb::new(Vec3::ZERO, Vec3::splat(10.0));
    assert_eq!(ray_at(0.0, 5.0).hit_aabb(&b), Some(4990.0));
    assert_eq!(ray_at(10.0, 10.0).hit_aabb(&b), Some(4990.0));
}

#[test]
fn empty_document_picks_nothing() {
    let doc = doc(vec![]);
    assert_eq!(pick(&doc, &limits(), &ray_at(100.0, 100.0)), None);
    assert_eq!(device_drop_target(&doc, &ray_at(100.0, 100.0), 0), None);
}

#[test]
fn pointer_on_the_far_edge_of_the_face_uses_the_last_cell() {
    let d = device("D", 1, 1);
    let did = d.id;
    let doc = doc(vec![rack("R", 42, vec![d])]);
    let face = device_face(0, &doc.racks[0].devices[0]);
    let t = port_drop_target(&doc, &limits(), did, &ray_at(face.max.x, face.max.y)).unwrap();
    assert_eq!(t.cell, Cell { row: 0, col: 4 });
}

#[test]
fn pick_hit_reports_where_the_ray_enters() {
    let d = device("D", 5, 1);
    let did = d.id;
    let doc = doc(vec![rack("R", 42, vec![d])]);
    let face = device_face(0, &doc.racks[0].devices[0]).center();
    let (id, at) = pick_hit(&doc, &limits(), &ray_at(face.x + 150.0, face.y)).unwrap();
    assert_eq!(id, ObjectId::Device(did));
    assert_eq!(at, Vec3::new(face.x + 150.0, face.y, 0.0));
}
