mod common;

use rackwright_core::model::{Gender, PortKind};

use common::*;
use glam::{Vec2, Vec3};
use rackwright_core::model::Document;
use rackwright_core::port_grid::Cell;
use rackwright_render::layout::*;

#[test]
fn racks_stand_side_by_side_with_a_gap() {
    assert_close(rack_left_x(0), 0.0);
    assert_close(rack_left_x(2), 2.0 * (RACK_WIDTH_MM + RACK_GAP_MM));
}

#[test]
fn u1_sits_on_the_plinth() {
    assert_close(u_bottom_y(1), PLINTH_MM);
    assert_close(u_bottom_y(3), PLINTH_MM + 2.0 * U_MM);
}

#[test]
fn device_box_spans_its_units_between_the_posts() {
    let d = device("PSU", 3, 2);
    let b = device_box(1, &d);
    let left = rack_left_x(1) + POST_WIDTH_MM;
    assert_close(b.min.x, left);
    assert_close(b.max.x, left + FRONT_WIDTH_MM);
    assert_close(b.min.y, u_bottom_y(3));
    assert_close(b.max.y, u_bottom_y(5));
    assert_close(b.max.z, 0.0);
    assert_close(b.min.z, -DEVICE_DEPTH_MM);
}

#[test]
fn rack_parts_cover_the_full_height_including_header() {
    let r = rack("R", 42, vec![]);
    let bounds = rack_parts(0, &r)
        .into_iter()
        .reduce(|a, b| a.union(&b))
        .unwrap();
    assert_close(bounds.min.y, 0.0);
    assert_close(bounds.max.y, PLINTH_MM + 42.0 * U_MM + HEADER_MM);
    assert_close(bounds.size().x, RACK_WIDTH_MM);
}

#[test]
fn rack_name_sits_on_the_header_above_the_top_unit() {
    let r = rack("R", 10, vec![]);
    let rect = rack_name_rect(0, &r);
    assert!(rect.min.y >= u_bottom_y(11));
    assert!(rect.max.y <= rack_total_height(&r));
    assert!(rect.z > 0.0);
}

#[test]
fn port_cells_divide_the_face_into_a_grid() {
    let d = device("D", 1, 2);
    let w = FRONT_WIDTH_MM / 5.0;
    let face = device_face(0, &d);
    let c = port_cell_rect(0, &d, Cell { row: 1, col: 4 }, &limits());
    assert_close(c.min.x, face.min.x + 4.0 * w);
    assert_close(c.max.x, face.max.x);
    assert_close(c.min.y, face.min.y + U_MM);
    assert_close(c.max.y, face.max.y);
}

#[test]
fn port_marker_protrudes_from_its_cell() {
    let d = device("D", 1, 1);
    let cell = Cell { row: 0, col: 2 };
    let rect = port_cell_rect(0, &d, cell, &limits());
    let marker = port_marker_box(
        0,
        &d,
        cell,
        PortKind::Unspecified,
        Gender::Unspecified,
        &limits(),
    );
    assert!(rect.contains(Vec2::new(marker.min.x, marker.min.y)));
    assert!(rect.contains(Vec2::new(marker.max.x, marker.max.y)));
    assert_close(marker.max.z, PORT_PROTRUSION_MM);
    let label = port_label_rect(0, &d, cell, &limits());
    assert!(label.max.y <= marker.min.y, "label sits under the marker");
}

#[test]
fn scene_bounds_cover_all_racks_or_a_default_rack() {
    let d = doc(vec![rack("A", 42, vec![]), rack("B", 20, vec![])]);
    let b = scene_bounds(&d);
    assert_close(b.min.x, 0.0);
    assert_close(b.max.x, rack_left_x(1) + RACK_WIDTH_MM);
    let empty = scene_bounds(&Document { racks: vec![], ..d });
    assert_close(empty.max.x, RACK_WIDTH_MM);
    assert_close(empty.max.y, PLINTH_MM + 42.0 * U_MM + HEADER_MM);
}

#[test]
fn aabb_helpers() {
    let a = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 4.0, 6.0));
    assert_eq!(a.center(), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(a.translated(Vec3::X).min, Vec3::X);
    let u = a.union(&Aabb::new(Vec3::splat(-1.0), Vec3::ONE));
    assert_eq!(
        (u.min, u.max),
        (Vec3::splat(-1.0), Vec3::new(2.0, 4.0, 6.0))
    );
}

#[test]
fn units_box_matches_a_device_at_the_same_place() {
    let d = device("D", 7, 3);
    assert_eq!(units_box(2, 7, 3), device_box(2, &d));
}

#[test]
fn object_bounds_finds_racks_devices_and_ports() {
    use rackwright_core::edit::ObjectId;
    let p = port("P", 0, 1);
    let pid = p.id;
    let dev = with_ports(device("D", 4, 2), vec![p]);
    let did = dev.id;
    let d = doc(vec![
        rack("A", 10, vec![]),
        rack("B", 10, vec![dev.clone()]),
    ]);
    let rid = d.racks[0].id;
    let l = limits();
    assert_eq!(
        object_bounds(&d, &l, ObjectId::Device(did)),
        Some(device_box(1, &dev))
    );
    let rack_b = object_bounds(&d, &l, ObjectId::Rack(rid)).unwrap();
    assert_close(rack_b.size().x, RACK_WIDTH_MM);
    let port_b = object_bounds(&d, &l, ObjectId::Port(pid)).unwrap();
    assert_eq!(
        port_b,
        port_marker_box(
            1,
            &dev,
            Cell { row: 0, col: 1 },
            PortKind::Unspecified,
            Gender::Unspecified,
            &l
        )
    );
    assert_eq!(
        object_bounds(
            &d,
            &l,
            ObjectId::Device(rackwright_core::ids::DeviceId::new())
        ),
        None
    );
}
