mod common;

use common::*;
use glam::Vec3;
use rackwright_core::edit::ObjectId;
use rackwright_core::model::Rgb;
use rackwright_render::layout::*;
use rackwright_render::scene::*;

fn input<'a>(
    d: &'a rackwright_core::model::Document,
    l: &'a rackwright_core::limits::Limits,
) -> SceneInput<'a> {
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

fn texts(scene: &Scene) -> Vec<&str> {
    scene.labels.iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn contrast_text_picks_black_or_white() {
    assert_eq!(
        contrast_text(Rgb {
            r: 250,
            g: 250,
            b: 250
        }),
        [0, 0, 0, 255]
    );
    assert_eq!(
        contrast_text(Rgb {
            r: 20,
            g: 20,
            b: 60
        }),
        [255, 255, 255, 255]
    );
}

#[test]
fn racks_devices_and_labels_are_drawn() {
    let mut dmm = device("DMM", 3, 2);
    dmm.color = Rgb { r: 255, g: 0, b: 0 };
    let d = doc(vec![rack("Rack1", 4, vec![dmm.clone()])]);
    let l = limits();
    let scene = build_scene(&input(&d, &l), &Motion::default());
    assert_eq!(scene.opaque.len(), 5 + 1, "five frame parts and one device");
    let device_box_drawn = scene
        .opaque
        .iter()
        .find(|b| b.aabb == device_box(0, &dmm))
        .unwrap();
    assert_eq!(device_box_drawn.color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(texts(&scene), vec!["Rack1", "1", "2", "3", "4", "DMM"]);
    assert!(scene.selected.is_empty() && scene.translucent.is_empty());
}

#[test]
fn port_labels_appear_only_when_hovered_or_selected() {
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let d = doc(vec![rack(
        "R",
        2,
        vec![with_ports(device("D", 1, 1), vec![p])],
    )]);
    let l = limits();
    let plain = build_scene(&input(&d, &l), &Motion::default());
    assert!(!texts(&plain).contains(&"CH1"));
    assert_eq!(plain.opaque.len(), 5 + 1 + 1, "port marker is drawn");
    let hovered = build_scene(
        &SceneInput {
            hovered_port: Some(pid),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert!(texts(&hovered).contains(&"CH1"));
    let selected = build_scene(
        &SceneInput {
            selection: Some(ObjectId::Port(pid)),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert!(texts(&selected).contains(&"CH1"));
    assert_eq!(selected.selected.len(), 1);
}

#[test]
fn selecting_a_rack_outlines_all_its_parts() {
    let d = doc(vec![rack("R", 2, vec![device("D", 1, 1)])]);
    let l = limits();
    let s = build_scene(
        &SceneInput {
            selection: Some(ObjectId::Rack(d.racks[0].id)),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(s.selected.len(), 5);
}

#[test]
fn ghost_is_translucent_and_red_when_invalid() {
    let d = doc(vec![]);
    let l = limits();
    let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
    let color = Rgb { r: 0, g: 0, b: 255 };
    let valid = build_scene(
        &SceneInput {
            ghost: Some(Ghost {
                aabb,
                color,
                valid: true,
            }),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(
        valid.translucent,
        vec![BoxInstance {
            aabb,
            color: rgba(color, GHOST_ALPHA)
        }]
    );
    let invalid = build_scene(
        &SceneInput {
            ghost: Some(Ghost {
                aabb,
                color,
                valid: false,
            }),
            ..input(&d, &l)
        },
        &Motion::default(),
    );
    assert_eq!(
        invalid.translucent[0].color,
        rgba(GHOST_INVALID, GHOST_ALPHA)
    );
}

#[test]
fn motion_eases_towards_new_positions() {
    let id = ObjectId::Device(rackwright_core::ids::DeviceId::new());
    let mut m = Motion::default();
    assert!(
        !m.update(&[(id, Vec3::ZERO)], 0.016),
        "new objects appear in place"
    );
    assert!(m.update(&[(id, Vec3::new(100.0, 0.0, 0.0))], 0.016));
    let offset = m.offset(id, Vec3::new(100.0, 0.0, 0.0));
    assert!(offset.x < 0.0 && offset.x > -100.0, "part-way: {offset}");
    for _ in 0..60 {
        m.update(&[(id, Vec3::new(100.0, 0.0, 0.0))], 0.016);
    }
    assert_eq!(m.offset(id, Vec3::new(100.0, 0.0, 0.0)), Vec3::ZERO);
}

#[test]
fn moving_devices_are_drawn_at_their_displayed_position() {
    let dev = device("D", 1, 1);
    let id = ObjectId::Device(dev.id);
    let d = doc(vec![rack("R", 10, vec![dev.clone()])]);
    let l = limits();
    let mut m = Motion::default();
    m.snap(&[(id, device_box(0, &dev).min - Vec3::new(0.0, 10.0, 0.0))]);
    let s = build_scene(&input(&d, &l), &m);
    let drawn = s.opaque.last().unwrap();
    assert_close(drawn.aabb.min.y, device_box(0, &dev).min.y - 10.0);
}

#[test]
fn motion_still_eases_after_an_idle_period() {
    let id = ObjectId::Device(rackwright_core::ids::DeviceId::new());
    let mut m = Motion::default();
    m.update(&[(id, Vec3::ZERO)], 0.016);
    assert!(
        m.update(&[(id, Vec3::new(100.0, 0.0, 0.0))], 2.0),
        "a long gap since the last frame must not skip the animation"
    );
}

#[test]
fn selecting_a_device_shows_all_its_port_names() {
    let d = with_ports(
        device("D", 1, 1),
        vec![port("CH1", 0, 0), port("CH2", 0, 1)],
    );
    let did = d.id;
    let other = with_ports(device("E", 5, 1), vec![port("X1", 0, 0)]);
    let doc = doc(vec![rack("R", 10, vec![d, other])]);
    let l = limits();
    let s = build_scene(
        &SceneInput {
            selection: Some(ObjectId::Device(did)),
            ..input(&doc, &l)
        },
        &Motion::default(),
    );
    let shown = texts(&s);
    assert!(
        shown.contains(&"CH1") && shown.contains(&"CH2"),
        "{shown:?}"
    );
    assert!(!shown.contains(&"X1"), "other devices' ports stay hidden");
}

#[test]
fn the_floor_extends_a_margin_beyond_every_rack() {
    let d = doc(vec![rack("A", 42, vec![]), rack("B", 24, vec![])]);
    let l = limits();
    let floor = build_scene(&input(&d, &l), &Motion::default())
        .floor
        .unwrap();
    let bounds = scene_bounds(&d);
    assert_eq!(
        floor.min,
        glam::Vec2::new(bounds.min.x, bounds.min.z) - FLOOR_MARGIN_MM
    );
    assert_eq!(
        floor.max,
        glam::Vec2::new(bounds.max.x, bounds.max.z) + FLOOR_MARGIN_MM
    );
}
