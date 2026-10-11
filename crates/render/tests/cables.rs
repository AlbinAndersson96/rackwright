mod common;

use rackwright_core::model::{Gender, PortKind};

use common::*;
use glam::Vec3;
use rackwright_core::edit::ObjectId;
use rackwright_core::ids::PortId;
use rackwright_core::model::{Document, Rgb};
use rackwright_core::port_grid::Cell;
use rackwright_render::layout::*;
use rackwright_render::pick::*;
use rackwright_render::scene::*;

/// R1: DMM (U10) with HI, LO; R2: PSU (U5) with OUT. Cable "Lead" joins HI and OUT.
fn station() -> (Document, PortId, PortId, PortId) {
    let dmm = with_ports(
        device("DMM", 10, 1),
        vec![port("HI", 0, 0), port("LO", 0, 4)],
    );
    let psu = with_ports(device("PSU", 5, 1), vec![port("OUT", 0, 2)]);
    let (hi, lo, out) = (dmm.ports[0].id, dmm.ports[1].id, psu.ports[0].id);
    let mut d = doc(vec![rack("R1", 42, vec![dmm]), rack("R2", 42, vec![psu])]);
    d.cables = vec![cable("Lead", hi, out)];
    (d, hi, lo, out)
}

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

fn ray_at(x: f32, y: f32) -> Ray {
    Ray {
        origin: Vec3::new(x, y, 5000.0),
        dir: Vec3::NEG_Z,
    }
}

#[test]
fn a_port_anchor_is_the_front_centre_of_its_marker() {
    let (d, hi, _, _) = station();
    let l = limits();
    let dmm = &d.racks[0].devices[0];
    let marker = port_marker_box(
        0,
        dmm,
        Cell { row: 0, col: 0 },
        PortKind::Unspecified,
        Gender::Unspecified,
        &l,
    );
    let anchor = port_anchor(&d, &l, hi).unwrap();
    assert_close(anchor.x, marker.center().x);
    assert_close(anchor.y, marker.center().y);
    assert_close(anchor.z, PORT_PROTRUSION_MM);
    assert_eq!(port_anchor(&d, &l, PortId::new()), None);
}

#[test]
fn a_cable_path_runs_from_end_to_end_in_front_of_the_racks_and_sags() {
    let a = Vec3::new(0.0, 1000.0, PORT_PROTRUSION_MM);
    let b = Vec3::new(800.0, 900.0, PORT_PROTRUSION_MM);
    let path = cable_path(a, b);
    assert_eq!(path.len(), CABLE_SEGMENTS + 1);
    assert_eq!(path[0], a);
    assert_eq!(*path.last().unwrap(), b);
    assert!(path.iter().all(|p| p.z >= PORT_PROTRUSION_MM - 1e-3));
    let middle = path[CABLE_SEGMENTS / 2];
    assert!(middle.z > 60.0, "leaves the front: {middle}");
    assert!(middle.y < 900.0, "sags below the lower end: {middle}");
    // Both ends leave straight towards the viewer.
    let first = (path[1] - path[0]).normalize();
    assert!(first.z > 0.7, "{first}");
}

#[test]
fn even_a_cable_between_neighbouring_ports_bulges_out() {
    let a = Vec3::new(0.0, 0.0, PORT_PROTRUSION_MM);
    let b = Vec3::new(10.0, 0.0, PORT_PROTRUSION_MM);
    let path = cable_path(a, b);
    assert!(path[CABLE_SEGMENTS / 2].z > 30.0);
}

#[test]
fn cables_are_drawn_as_tubes_between_their_ports() {
    let (d, hi, _, out) = station();
    let l = limits();
    let scene = build_scene(&input(&d, &l), &Motion::default());
    assert_eq!(scene.tubes.len(), CABLE_SEGMENTS);
    assert!(scene.selected_tubes.is_empty());
    let (from, to) = (
        port_anchor(&d, &l, hi).unwrap(),
        port_anchor(&d, &l, out).unwrap(),
    );
    assert_eq!(scene.tubes[0].a, from);
    assert_eq!(scene.tubes[CABLE_SEGMENTS - 1].b, to);
    assert_eq!(scene.tubes[0].radius, CABLE_RADIUS_MM);
    assert_eq!(scene.tubes[0].color, rgba(Rgb::CABLE_BLUE, 1.0));
    for pair in scene.tubes.windows(2) {
        assert_eq!(pair[0].b, pair[1].a, "segments join up");
    }
}

#[test]
fn a_selected_cable_is_outlined_and_shows_its_port_names() {
    let (d, _, _, _) = station();
    let l = limits();
    let id = ObjectId::Cable(d.cables[0].id);
    let scene = build_scene(
        &SceneInput {
            selection: Some(id),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(scene.selected_tubes, scene.tubes);
    let texts: Vec<&str> = scene.labels.iter().map(|l| l.text.as_str()).collect();
    assert!(texts.contains(&"HI") && texts.contains(&"OUT"), "{texts:?}");
    assert!(!texts.contains(&"LO"));
}

#[test]
fn cable_ends_follow_their_ports_motion() {
    let (d, hi, _, _) = station();
    let l = limits();
    let mut motion = Motion::default();
    motion.snap(&animation_targets(&d, &l));
    // Move HI one cell to the right; the displayed marker eases from the old position.
    let mut moved = d.clone();
    moved.racks[0].devices[0].ports[0].col = 1;
    motion.update(&animation_targets(&moved, &l), 0.0);
    let scene = build_scene(&input(&moved, &l), &motion);
    assert_eq!(scene.tubes[0].a, port_anchor(&d, &l, hi).unwrap());
}

#[test]
fn the_cable_preview_is_drawn_and_turns_red_when_invalid() {
    let (d, _, _, _) = station();
    let l = limits();
    let from = Vec3::new(0.0, 500.0, PORT_PROTRUSION_MM);
    let to = Vec3::new(300.0, 400.0, 0.0);
    let with = |valid| {
        build_scene(
            &SceneInput {
                cable_preview: Some(CablePreview {
                    from,
                    to,
                    color: Rgb::CABLE_BLUE,
                    valid,
                }),
                ..input(&d, &l)
            },
            &Motion::default(),
        )
    };
    let ok = with(true);
    assert_eq!(ok.tubes.len(), 2 * CABLE_SEGMENTS);
    let preview = &ok.tubes[CABLE_SEGMENTS..];
    assert_eq!(preview[0].a, from);
    assert_eq!(preview[CABLE_SEGMENTS - 1].b, to);
    assert_eq!(preview[0].color, rgba(Rgb::CABLE_BLUE, 1.0));
    let bad = with(false);
    assert_eq!(bad.tubes[CABLE_SEGMENTS].color, rgba(GHOST_INVALID, 1.0));
}

#[test]
fn cables_can_be_picked_and_framed() {
    let (d, _, _, _) = station();
    let l = limits();
    let id = ObjectId::Cable(d.cables[0].id);
    let path = cable_points(&d, &l, &d.cables[0]).unwrap();
    let middle = path[CABLE_SEGMENTS / 2];
    assert_eq!(pick(&d, &l, &ray_at(middle.x, middle.y)), Some(id));
    // Just beside the tube is still a hit; far beside it is not.
    assert_eq!(
        pick(&d, &l, &ray_at(middle.x, middle.y + CABLE_RADIUS_MM * 1.8)),
        Some(id)
    );
    assert_ne!(pick(&d, &l, &ray_at(middle.x, middle.y + 60.0)), Some(id));

    let bounds = object_bounds(&d, &l, id).unwrap();
    for p in &path {
        assert!(bounds.min.cmple(*p).all() && bounds.max.cmpge(*p).all());
    }
}

#[test]
fn a_connected_port_is_still_picked_over_its_cable() {
    let (d, hi, _, _) = station();
    let l = limits();
    let anchor = port_anchor(&d, &l, hi).unwrap();
    assert_eq!(
        pick(&d, &l, &ray_at(anchor.x, anchor.y)),
        Some(ObjectId::Port(hi))
    );
}

#[test]
fn ray_to_segment_distance() {
    let ray = ray_at(0.0, 0.0);
    let (distance, t) = ray
        .closest_to_segment(Vec3::new(-10.0, 3.0, 100.0), Vec3::new(10.0, 3.0, 100.0))
        .unwrap();
    assert_close(distance, 3.0);
    assert_close(t, 4900.0);
    // Beyond the segment's end, the end point is the closest.
    let (distance, _) = ray
        .closest_to_segment(Vec3::new(5.0, 0.0, 0.0), Vec3::new(9.0, 0.0, 0.0))
        .unwrap();
    assert_close(distance, 5.0);
    // Behind the ray's origin there is nothing.
    assert_eq!(
        ray.closest_to_segment(Vec3::new(0.0, 0.0, 6000.0), Vec3::new(1.0, 0.0, 6000.0)),
        None
    );
}

#[test]
fn a_preview_keeps_its_colour_unless_invalid() {
    let (d, _, _, _) = station();
    let l = limits();
    let green = Rgb { r: 0, g: 200, b: 0 };
    let preview = CablePreview {
        from: Vec3::new(0.0, 500.0, PORT_PROTRUSION_MM),
        to: Vec3::new(300.0, 400.0, 0.0),
        color: green,
        valid: true,
    };
    let scene = build_scene(
        &SceneInput {
            cable_preview: Some(preview),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(scene.tubes[CABLE_SEGMENTS].color, rgba(green, 1.0));
}

#[test]
fn a_hidden_cable_is_not_drawn_but_its_ports_are() {
    let (d, _, _, _) = station();
    let l = limits();
    let shown = build_scene(&input(&d, &l), &Motion::default());
    let hidden = build_scene(
        &SceneInput {
            hidden_cable: Some(d.cables[0].id),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert!(hidden.tubes.is_empty());
    assert_eq!(hidden.opaque, shown.opaque);
}
