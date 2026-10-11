mod common;

use rackwright_core::model::{Gender, PortKind};

use common::*;
use glam::{Vec2, Vec3};
use rackwright_app::interaction::{Button, DragSource};
use rackwright_app::session::Session;
use rackwright_core::edit::ObjectId;
use rackwright_core::model::Document;
use rackwright_core::port_grid::Cell;
use rackwright_render::layout::{
    RACK_WIDTH_MM, U_MM, device_face, port_cell_rect, port_marker_box, rack_left_x, u_bottom_y,
};

/// Centre of the front face of the device named `n`, at its bottom unit.
fn device_point(s: &Session, n: &str) -> Vec3 {
    let (ri, d) = s
        .document()
        .racks
        .iter()
        .enumerate()
        .find_map(|(ri, r)| {
            r.devices
                .iter()
                .find(|d| d.name.as_str() == n)
                .map(|d| (ri, d))
        })
        .unwrap();
    let f = device_face(ri, d);
    Vec3::new((f.min.x + f.max.x) / 2.0, f.min.y + U_MM / 2.0, 0.0)
}

/// A point on rack 0's front plane at the middle of unit `u`.
fn unit_point(u: u32) -> Vec3 {
    let x = rack_left_x(0) + RACK_WIDTH_MM / 2.0;
    Vec3::new(x, u_bottom_y(u) + U_MM / 2.0, 0.0)
}

fn drag(s: &mut Session, from: Vec2, to: Vec2, over_trash: bool) {
    s.pointer_down(from, Button::Left);
    s.pointer_move(from + Vec2::new(5.0, 0.0), false, 0.0);
    s.pointer_move(to, over_trash, 0.0);
    s.pointer_up(to, over_trash, 0.0);
}

fn bottom_of(s: &Session, n: &str) -> u32 {
    s.document()
        .racks
        .iter()
        .flat_map(|r| &r.devices)
        .find(|d| d.name.as_str() == n)
        .unwrap()
        .bottom_u
}

#[test]
fn clicking_selects_and_clicking_empty_space_clears() {
    let d = device("DMM", 10, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let p = px(&s, device_point(&s, "DMM"));
    s.pointer_down(p, Button::Left);
    s.pointer_up(p, false, 0.0);
    assert_eq!(s.selection(), Some(ObjectId::Device(did)));
    s.pointer_down(Vec2::new(2.0, 2.0), Button::Left);
    s.pointer_up(Vec2::new(2.0, 2.0), false, 0.0);
    assert_eq!(s.selection(), None);
}

#[test]
fn dragging_empty_space_orbits_and_middle_drag_pans() {
    let mut s = session(Document::new_default(&limits()));
    let before = *s.camera();
    drag(&mut s, Vec2::new(5.0, 5.0), Vec2::new(105.0, 5.0), false);
    assert_ne!(s.camera().yaw, before.yaw);
    assert_eq!(s.selection(), None);

    let before = *s.camera();
    s.pointer_down(Vec2::new(400.0, 300.0), Button::Middle);
    s.pointer_move(Vec2::new(450.0, 300.0), false, 0.0);
    s.pointer_up(Vec2::new(450.0, 300.0), false, 0.0);
    assert_ne!(s.camera().target, before.target);
}

#[test]
fn dragging_a_device_previews_then_commits() {
    let mut s = session(doc(vec![rack(
        "R",
        42,
        vec![device("A", 10, 1), device("B", 20, 1)],
    )]));
    let from = px(&s, device_point(&s, "A"));
    let to = px(&s, unit_point(20));
    s.pointer_down(from, Button::Left);
    s.pointer_move(from + Vec2::new(5.0, 0.0), false, 0.0);
    s.pointer_move(to, false, 0.0);
    assert!(s.is_dragging());
    let ghost = s.ghost().unwrap();
    assert!(ghost.valid);
    let shown_b = s.shown_document().racks[0]
        .devices
        .iter()
        .find(|d| d.name.as_str() == "B")
        .unwrap();
    assert_eq!(shown_b.bottom_u, 19, "preview pushes B down");
    assert_eq!(bottom_of(&s, "B"), 20, "not committed yet");
    s.pointer_up(to, false, 0.0);
    assert_eq!((bottom_of(&s, "A"), bottom_of(&s, "B")), (20, 19));
    assert!(s.ghost().is_none() && !s.is_dragging());
    s.undo();
    assert_eq!((bottom_of(&s, "A"), bottom_of(&s, "B")), (10, 20));
}

#[test]
fn rejected_drop_shows_a_red_ghost_and_changes_nothing() {
    let mut s = session(doc(vec![rack(
        "R",
        42,
        vec![device("A", 10, 1), device("Low", 1, 1)],
    )]));
    let before = s.document().clone();
    let from = px(&s, device_point(&s, "A"));
    let to = px(&s, unit_point(1));
    s.pointer_down(from, Button::Left);
    s.pointer_move(from + Vec2::new(5.0, 0.0), false, 0.0);
    s.pointer_move(to, false, 0.0);
    assert_eq!(s.ghost().map(|g| g.valid), Some(false));
    s.pointer_up(to, false, 0.0);
    assert_eq!(s.document(), &before);
}

#[test]
fn dropping_on_the_trash_deletes() {
    let mut s = session(doc(vec![rack("R", 42, vec![device("A", 10, 1)])]));
    let from = px(&s, device_point(&s, "A"));
    drag(&mut s, from, Vec2::new(780.0, 580.0), true);
    assert!(s.document().racks[0].devices.is_empty());
}

#[test]
fn new_device_handle_drop_creates_and_selects_it() {
    let mut s = session(doc(vec![rack("R", 42, vec![device("DMM", 1, 1)])]));
    s.start_drag(
        DragSource::NewDevice {
            name: name("dmm"),
            height_u: 2,
        },
        0.0,
    );
    let to = px(&s, unit_point(30));
    s.pointer_move(to, false, 0.0);
    s.pointer_up(to, false, 0.0);
    assert_eq!(bottom_of(&s, "dmm_2"), 30, "auto-renamed and placed");
    let created = s.document().racks[0]
        .devices
        .iter()
        .find(|d| d.name.as_str() == "dmm_2")
        .unwrap()
        .id;
    assert_eq!(s.selection(), Some(ObjectId::Device(created)));
}

#[test]
fn new_device_dropped_on_trash_or_cancelled_changes_nothing() {
    let mut s = session(Document::new_default(&limits()));
    let before = s.document().clone();
    s.start_drag(
        DragSource::NewDevice {
            name: name("X"),
            height_u: 1,
        },
        0.0,
    );
    let to = px(&s, unit_point(5));
    s.pointer_move(to, true, 0.0);
    s.pointer_up(to, true, 0.0);
    assert_eq!(s.document(), &before);

    s.start_drag(
        DragSource::NewDevice {
            name: name("X"),
            height_u: 1,
        },
        0.0,
    );
    s.pointer_move(to, false, 0.0);
    assert!(s.ghost().is_some());
    s.escape(0.0);
    assert!(!s.is_dragging() && s.ghost().is_none());
    assert_eq!(s.document(), &before);
}

#[test]
fn port_drags_zoom_to_the_face_and_back() {
    let d = with_ports(device("D", 10, 1), vec![port("A", 0, 0)]);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    s.select(Some(ObjectId::Device(did)));
    let start = *s.camera();
    let mut now = 0.0;
    s.start_drag(
        DragSource::NewPort {
            device: did,
            name: name("B"),
            kind: PortKind::Unspecified,
            gender: Gender::Unspecified,
        },
        now,
    );
    settle(&mut s, &mut now);
    assert_eq!(s.camera().pitch, 0.0, "straight-on face view");
    let dev = s.document().racks[0].devices[0].clone();
    let target = port_cell_rect(0, &dev, Cell { row: 0, col: 3 }, &limits()).center();
    let p = px(&s, target);
    s.pointer_move(p, false, now);
    s.pointer_up(p, false, now);
    settle(&mut s, &mut now);
    assert_eq!(*s.camera(), start, "camera returned");
    let ports: Vec<(String, u32)> = s.document().racks[0].devices[0]
        .ports
        .iter()
        .map(|p| (p.name.to_string(), p.col))
        .collect();
    assert_eq!(ports, vec![("A".to_string(), 0), ("B".to_string(), 3)]);
}

#[test]
fn hovering_a_port_marks_it() {
    let p = port("CH1", 0, 2);
    let pid = p.id;
    let d = with_ports(device("D", 10, 1), vec![p]);
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let dev = s.document().racks[0].devices[0].clone();
    let marker = port_marker_box(
        0,
        &dev,
        Cell { row: 0, col: 2 },
        PortKind::Unspecified,
        Gender::Unspecified,
        &limits(),
    );
    let at = px(
        &s,
        Vec3::new(marker.center().x, marker.center().y, marker.max.z),
    );
    s.pointer_move(at, false, 0.0);
    assert_eq!(s.hovered_port(), Some(pid));
    s.pointer_move(Vec2::new(2.0, 2.0), false, 0.0);
    assert_eq!(s.hovered_port(), None);
}

#[test]
fn wheel_zooms_and_context_target_picks() {
    let d = device("D", 10, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let before = s.camera().distance;
    s.wheel(-100.0);
    assert!(s.camera().distance < before);
    let p = px(&s, device_point(&s, "D"));
    assert_eq!(s.context_target(p), Some(ObjectId::Device(did)));
    assert_eq!(s.context_target(Vec2::new(2.0, 2.0)), None);
}

#[test]
fn undo_is_ignored_during_a_drag() {
    let mut s = session(Document::new_default(&limits()));
    s.add_rack();
    s.start_drag(
        DragSource::NewDevice {
            name: name("X"),
            height_u: 1,
        },
        0.0,
    );
    s.undo();
    assert_eq!(s.document().racks.len(), 2);
}

#[test]
fn pointer_cancel_ends_any_gesture_without_changes() {
    let mut s = session(doc(vec![rack("R", 42, vec![device("A", 10, 1)])]));
    let before = s.document().clone();
    let from = px(&s, device_point(&s, "A"));
    s.pointer_down(from, Button::Left);
    s.pointer_move(px(&s, unit_point(30)), false, 0.0);
    assert!(s.is_dragging());
    s.pointer_cancel(0.0);
    assert!(!s.is_dragging() && s.ghost().is_none());
    assert_eq!(s.document(), &before);
    s.pointer_down(Vec2::new(5.0, 5.0), Button::Left);
    s.pointer_move(Vec2::new(60.0, 5.0), false, 0.0);
    s.pointer_cancel(0.0);
    s.pointer_down(Vec2::new(5.0, 5.0), Button::Left);
    s.pointer_up(Vec2::new(5.0, 5.0), false, 0.0);
    assert_eq!(
        s.selection(),
        None,
        "a fresh click works after a cancelled orbit"
    );
}

#[test]
fn new_device_released_outside_any_rack_changes_nothing() {
    let mut s = session(Document::new_default(&limits()));
    let before = s.document().clone();
    s.start_drag(
        DragSource::NewDevice {
            name: name("X"),
            height_u: 1,
        },
        0.0,
    );
    s.pointer_move(Vec2::new(2.0, 2.0), false, 0.0);
    assert!(s.ghost().is_none());
    s.pointer_up(Vec2::new(2.0, 2.0), false, 0.0);
    assert_eq!(s.document(), &before);
}

#[test]
fn undoing_away_a_hovered_port_clears_the_hover() {
    let d = device("D", 10, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let mut now = 0.0;
    s.start_drag(
        DragSource::NewPort {
            device: did,
            name: name("P"),
            kind: PortKind::Unspecified,
            gender: Gender::Unspecified,
        },
        now,
    );
    settle(&mut s, &mut now);
    let dev = s.document().racks[0].devices[0].clone();
    let p = px(
        &s,
        port_cell_rect(0, &dev, Cell { row: 0, col: 1 }, &limits()).center(),
    );
    s.pointer_move(p, false, now);
    s.pointer_up(p, false, now);
    settle(&mut s, &mut now);
    let dev = s.document().racks[0].devices[0].clone();
    let marker = port_marker_box(
        0,
        &dev,
        Cell { row: 0, col: 1 },
        PortKind::Unspecified,
        Gender::Unspecified,
        &limits(),
    );
    let at = px(
        &s,
        Vec3::new(marker.center().x, marker.center().y, marker.max.z),
    );
    s.pointer_move(at, false, now);
    assert!(s.hovered_port().is_some());
    s.undo();
    assert_eq!(s.hovered_port(), None);
}

#[test]
fn drops_outside_the_canvas_never_commit() {
    let mut s = session(doc(vec![rack("R", 42, vec![device("A", 10, 1)])]));
    // Pan so the rack's centre sits 50 px left of the canvas edge.
    let centre = px(&s, unit_point(20));
    s.pointer_down(Vec2::new(400.0, 300.0), Button::Middle);
    s.pointer_move(Vec2::new(400.0 - centre.x - 50.0, 300.0), false, 0.0);
    s.pointer_up(Vec2::new(400.0 - centre.x - 50.0, 300.0), false, 0.0);
    let off_canvas = px(&s, unit_point(20));
    assert!(
        off_canvas.x < 0.0,
        "rack centre is off-canvas: {off_canvas}"
    );
    assert!(
        rackwright_render::pick::device_drop_target(s.document(), &s.ray_at(off_canvas), 0)
            .is_some(),
        "the ray there still hits the rack"
    );
    let before = s.document().clone();
    s.start_drag(
        DragSource::NewDevice {
            name: name("X"),
            height_u: 1,
        },
        0.0,
    );
    s.pointer_move(off_canvas, false, 0.0);
    assert!(s.ghost().is_none());
    s.pointer_up(off_canvas, false, 0.0);
    assert_eq!(s.document(), &before);
}

#[test]
fn is_busy_reports_any_gesture_in_progress() {
    let mut s = session(Document::new_default(&limits()));
    assert!(!s.is_busy());
    s.pointer_down(Vec2::new(5.0, 5.0), Button::Left);
    assert!(s.is_busy());
    s.pointer_cancel(0.0);
    assert!(!s.is_busy());
}

#[test]
fn double_click_selects_and_frames_the_object() {
    let d = device("DMM", 20, 1);
    let did = d.id;
    let mut s = session(doc(vec![rack("R", 42, vec![d])]));
    let start = *s.camera();
    let mut now = 1.0;
    let p = px(&s, device_point(&s, "DMM"));
    s.double_click(p, now);
    assert_eq!(s.selection(), Some(ObjectId::Device(did)));
    settle(&mut s, &mut now);
    assert!(
        s.camera().distance < start.distance,
        "zoomed in on the device"
    );

    let framed = *s.camera();
    s.double_click(Vec2::new(2.0, 2.0), now);
    settle(&mut s, &mut now);
    assert_eq!(*s.camera(), framed, "empty space does nothing");
    assert_eq!(s.selection(), Some(ObjectId::Device(did)));
}
