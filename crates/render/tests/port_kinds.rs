mod common;

use common::*;
use glam::Vec3;
use rackwright_core::edit::ObjectId;
use rackwright_core::model::{Document, Gender, PortKind};
use rackwright_core::port_grid::Cell;
use rackwright_render::layout::*;
use rackwright_render::pick::*;
use rackwright_render::scene::*;

fn input<'a>(d: &'a Document, l: &'a rackwright_core::limits::Limits) -> SceneInput<'a> {
    SceneInput {
        document: d,
        limits: l,
        selection: None,
        hovered_port: None,
        ghost: None,
        cable_preview: None,
        hidden_cable: None,
    }
}

/// One device with a single port of `kind` in cell (0, 2).
fn one_port(kind: PortKind) -> Document {
    let mut p = port("P", 0, 2);
    p.kind = kind;
    doc(vec![rack(
        "R",
        42,
        vec![with_ports(device("D", 10, 1), vec![p])],
    )])
}

#[test]
fn markers_take_their_type_shape_around_the_same_centre() {
    let d = one_port(PortKind::Unspecified);
    let dev = &d.racks[0].devices[0];
    let l = limits();
    let cell = Cell { row: 0, col: 2 };
    let square = port_marker_box(0, dev, cell, PortKind::Unspecified, Gender::Unspecified, &l);
    let side = square.size().x;
    assert_close(square.size().y, side);
    for kind in PortKind::ALL {
        let b = port_marker_box(0, dev, cell, kind, Gender::Unspecified, &l);
        assert_close(b.center().x, square.center().x);
        assert_close(b.center().y, square.center().y);
        assert_close(b.size().z, PORT_PROTRUSION_MM);
        let (w, h) = match marker_shape(kind) {
            MarkerShape::Square => (1.0, 1.0),
            MarkerShape::Round(d) => (d, d),
            MarkerShape::Rect(w, h) => (w, h),
        };
        assert_close(b.size().x, side * w);
        assert_close(b.size().y, side * h);
        // Never wider than the cell.
        assert!(
            b.size().x < port_cell_rect(0, dev, cell, &l).size().x,
            "{kind:?}"
        );
    }
    assert_eq!(marker_shape(PortKind::Bnc), MarkerShape::Round(1.0));
    assert_eq!(marker_shape(PortKind::Gpib), MarkerShape::Rect(1.9, 0.5));
    assert_eq!(marker_shape(PortKind::Other), MarkerShape::Square);
}

#[test]
fn round_markers_are_drawn_as_short_tubes_in_their_colour() {
    let d = one_port(PortKind::Bnc);
    let l = limits();
    let pid = d.racks[0].devices[0].ports[0].id;
    let marker = object_bounds(&d, &l, ObjectId::Port(pid)).unwrap();
    let scene = build_scene(
        &SceneInput {
            selection: Some(ObjectId::Port(pid)),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(scene.tubes.len(), 1);
    let t = scene.tubes[0];
    let c = marker.center();
    assert_eq!(t.a, Vec3::new(c.x, c.y, marker.min.z));
    assert_eq!(t.b, Vec3::new(c.x, c.y, marker.max.z));
    assert_close(t.radius, marker.size().x / 2.0);
    assert_eq!(t.color, rgba(port_color(PortKind::Bnc).unwrap(), 1.0));
    assert_eq!(scene.selected_tubes, vec![t]);
    assert!(
        scene.opaque.iter().all(|b| b.aabb != marker),
        "no box for it"
    );
}

#[test]
fn other_markers_are_boxes_in_their_colour_or_the_device_ink() {
    let l = limits();
    for (kind, expected) in [
        (PortKind::Lan, port_color(PortKind::Lan)),
        (PortKind::Unspecified, None),
        (PortKind::Other, None),
    ] {
        let d = one_port(kind);
        let pid = d.racks[0].devices[0].ports[0].id;
        let marker = object_bounds(&d, &l, ObjectId::Port(pid)).unwrap();
        let scene = build_scene(&input(&d, &l), &Motion::default());
        assert!(scene.tubes.is_empty());
        let drawn = scene.opaque.iter().find(|b| b.aabb == marker).unwrap();
        let ink = contrast_text(d.racks[0].devices[0].color);
        let ink = rackwright_core::model::Rgb {
            r: ink[0],
            g: ink[1],
            b: ink[2],
        };
        assert_eq!(drawn.color, rgba(expected.unwrap_or(ink), 1.0), "{kind:?}");
    }
}

#[test]
fn shaped_markers_are_picked_by_their_shape() {
    let d = one_port(PortKind::Gpib);
    let l = limits();
    let pid = d.racks[0].devices[0].ports[0].id;
    let marker = object_bounds(&d, &l, ObjectId::Port(pid)).unwrap();
    // Near the wide marker's right end, outside where a square marker would be.
    let x = marker.max.x - 2.0;
    let ray = Ray {
        origin: Vec3::new(x, marker.center().y, 5000.0),
        dir: Vec3::NEG_Z,
    };
    assert_eq!(pick(&d, &l, &ray), Some(ObjectId::Port(pid)));
}

#[test]
fn gender_sets_how_far_a_marker_sticks_out() {
    let d = one_port(PortKind::Bnc);
    let dev = &d.racks[0].devices[0];
    let l = limits();
    let cell = Cell { row: 0, col: 2 };
    let depth = |g| port_marker_box(0, dev, cell, PortKind::Bnc, g, &l).size().z;
    assert_close(depth(Gender::Unspecified), PORT_PROTRUSION_MM);
    assert_close(depth(Gender::Other), PORT_PROTRUSION_MM);
    assert_close(depth(Gender::Male), PORT_PROTRUSION_MM * 1.6);
    assert_close(depth(Gender::Female), PORT_PROTRUSION_MM * 0.5);
    let flat = port_marker_box(0, dev, cell, PortKind::Bnc, Gender::Female, &l);
    let square = port_marker_box(0, dev, cell, PortKind::Bnc, Gender::Unspecified, &l);
    assert_eq!(
        (flat.min.x, flat.max.x),
        (square.min.x, square.max.x),
        "same outline"
    );

    // The scene and the cable anchor follow the port's gender.
    let mut male = d.clone();
    male.racks[0].devices[0].ports[0].gender = Gender::Male;
    let pid = male.racks[0].devices[0].ports[0].id;
    let anchor = port_anchor(&male, &l, pid).unwrap();
    assert_close(anchor.z, PORT_PROTRUSION_MM * 1.6);
    let scene = build_scene(&input(&male, &l), &Motion::default());
    assert_close(scene.tubes[0].b.z, PORT_PROTRUSION_MM * 1.6);
}
