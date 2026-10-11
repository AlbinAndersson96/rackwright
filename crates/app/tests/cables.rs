mod common;

use common::*;
use glam::Vec2;
use rackwright_app::interaction::{Button, DragSource};
use rackwright_app::session::Session;
use rackwright_core::edit::ObjectId;
use rackwright_core::ids::PortId;
use rackwright_core::model::{Document, Rgb};
use rackwright_render::layout::{CABLE_SEGMENTS, cable_points, port_anchor};

/// R1: DMM (U20) with HI, LO, SENSE; PSU (U18) with OUT. The ports are spread out so each is
/// well clear of the others on screen.
fn station() -> (Document, [PortId; 4]) {
    let dmm = with_ports(
        device("DMM", 20, 1),
        vec![port("HI", 0, 0), port("LO", 0, 2), port("SENSE", 0, 4)],
    );
    let psu = with_ports(device("PSU", 18, 1), vec![port("OUT", 0, 3)]);
    let ids = [
        dmm.ports[0].id,
        dmm.ports[1].id,
        dmm.ports[2].id,
        psu.ports[0].id,
    ];
    (doc(vec![rack("R1", 42, vec![dmm, psu])]), ids)
}

fn port_px(s: &Session, p: PortId) -> Vec2 {
    px(s, port_anchor(s.document(), &s.limits, p).unwrap())
}

/// Shift-drags from port `from` to canvas position `to` and releases there.
fn connect_drag(s: &mut Session, from: PortId, to: Vec2) {
    let start = port_px(s, from);
    s.pointer_down_with_shift(start, Button::Left, true);
    s.pointer_move(start + Vec2::new(5.0, 0.0), false, 0.0);
    s.pointer_move(to, false, 0.0);
    s.pointer_up(to, false, 0.0);
}

#[test]
fn shift_dragging_from_port_to_port_connects_them() {
    let (d, [hi, _, _, out]) = station();
    let mut s = session(d);
    let to = port_px(&s, out);
    connect_drag(&mut s, hi, to);
    let cables = &s.document().cables;
    assert_eq!(cables.len(), 1);
    assert_eq!((cables[0].a, cables[0].b), (hi, out));
    assert_eq!(s.selection(), Some(ObjectId::Cable(cables[0].id)));
    // One undo step; the selection goes with the cable.
    s.undo();
    assert!(s.document().cables.is_empty());
    assert_eq!(s.selection(), None);
    s.redo();
    assert_eq!(s.document().cables.len(), 1);
}

#[test]
fn a_plain_drag_still_moves_the_port() {
    let (d, [hi, _, _, out]) = station();
    let mut s = session(d);
    let (from, to) = (port_px(&s, hi), port_px(&s, out));
    s.pointer_down(from, Button::Left);
    s.pointer_move(from + Vec2::new(5.0, 0.0), false, 0.0);
    assert!(s.is_dragging(), "a port move shows the trash zone");
    s.pointer_up(to, false, 0.0);
    assert!(s.document().cables.is_empty());
}

#[test]
fn the_preview_follows_the_pointer_and_snaps_to_a_target_port() {
    let (d, [hi, _, sense, out]) = station();
    let mut s = session(d);
    let camera = *s.camera();
    let start = port_px(&s, hi);
    s.pointer_down_with_shift(start, Button::Left, true);
    s.pointer_move(start + Vec2::new(5.0, 0.0), false, 0.0);
    assert!(!s.is_dragging(), "no trash zone for cables");

    // Over empty space below the racks: a loose preview, nothing planned.
    s.pointer_move(Vec2::new(400.0, 590.0), false, 0.0);
    let preview = s.cable_preview().expect("loose preview");
    assert!(preview.valid);
    assert_eq!(
        preview.from,
        port_anchor(s.document(), &s.limits, hi).unwrap()
    );
    assert!(s.shown_document().cables.is_empty());
    assert_eq!(s.scene().tubes.len(), CABLE_SEGMENTS);

    // Over a free port: the planned cable is shown, and the port's name.
    s.pointer_move(port_px(&s, out), false, 0.0);
    assert_eq!(s.cable_preview(), None);
    assert_eq!(s.shown_document().cables.len(), 1);
    assert_eq!(s.hovered_port(), Some(out));
    assert_eq!(s.scene().tubes.len(), CABLE_SEGMENTS);
    assert!(s.document().cables.is_empty(), "nothing committed yet");

    // Over the start port itself: a neutral loose preview.
    s.pointer_move(port_px(&s, hi), false, 0.0);
    assert!(s.cable_preview().is_some_and(|p| p.valid));
    assert!(s.shown_document().cables.is_empty());

    s.pointer_move(port_px(&s, sense), false, 0.0);
    s.escape(0.0);
    assert!(s.document().cables.is_empty());
    assert_eq!(s.cable_preview(), None);
    assert_eq!(s.scene().tubes.len(), 0);
    assert_eq!(*s.camera(), camera, "the camera never moved");
}

#[test]
fn a_port_with_a_cable_cannot_take_another() {
    let (mut d, [hi, lo, _, out]) = station();
    d.cables = vec![cable("C1", hi, out)];
    let mut s = session(d);
    let start = port_px(&s, lo);
    s.pointer_down_with_shift(start, Button::Left, true);
    s.pointer_move(start + Vec2::new(5.0, 0.0), false, 0.0);
    s.pointer_move(port_px(&s, out), false, 0.0);
    let preview = s.cable_preview().expect("red preview");
    assert!(!preview.valid);
    assert_eq!(
        preview.to,
        port_anchor(s.document(), &s.limits, out).unwrap()
    );
    s.pointer_up(port_px(&s, out), false, 0.0);
    assert_eq!(s.document().cables.len(), 1);
    assert_eq!(s.revision(), 0, "nothing changed");
}

#[test]
fn releasing_elsewhere_changes_nothing() {
    let (d, [hi, ..]) = station();
    let mut s = session(d);
    connect_drag(&mut s, hi, Vec2::new(400.0, 590.0));
    assert!(s.document().cables.is_empty());
    assert_eq!(s.revision(), 0);
    assert!(!s.is_busy());
}

#[test]
fn the_port_panel_handle_starts_the_same_drag() {
    let (d, [_, lo, sense, _]) = station();
    let mut s = session(d);
    s.start_drag(DragSource::Cable { from: lo }, 0.0);
    let to = port_px(&s, sense);
    s.pointer_move(to, false, 0.0);
    s.pointer_up(to, false, 0.0);
    let c = &s.document().cables[0];
    assert_eq!((c.a, c.b), (lo, sense));
}

#[test]
fn cables_can_be_clicked_renamed_recoloured_and_deleted() {
    let (mut d, [hi, _, _, out]) = station();
    d.cables = vec![cable("C1", hi, out)];
    let id = d.cables[0].id;
    let mut s = session(d);
    let path = cable_points(s.document(), &s.limits, &s.document().cables[0]).unwrap();
    let middle = px(&s, path[CABLE_SEGMENTS / 2]);
    s.pointer_down(middle, Button::Left);
    s.pointer_up(middle, false, 0.0);
    assert_eq!(s.selection(), Some(ObjectId::Cable(id)));

    assert_eq!(s.rename(ObjectId::Cable(id), "Sense +"), Ok(()));
    assert_eq!(s.document().cables[0].name.as_str(), "Sense+");
    let red = Rgb { r: 255, g: 0, b: 0 };
    s.set_cable_color(id, red);
    assert_eq!(s.document().cables[0].color, red);

    s.delete(ObjectId::Cable(id));
    assert!(s.document().cables.is_empty());
    assert_eq!(s.selection(), None);
    assert_eq!(s.document().racks[0].devices[0].ports.len(), 3);
}

#[test]
fn deleting_a_port_takes_its_cable_and_undo_brings_both_back() {
    let (mut d, [hi, _, _, out]) = station();
    d.cables = vec![cable("C1", hi, out)];
    let mut s = session(d);
    s.delete(ObjectId::Port(out));
    assert!(s.document().cables.is_empty());
    s.undo();
    assert_eq!(s.document().cables.len(), 1);
    assert!(s.document().port(out).is_some());
}

#[test]
fn cables_are_exported_and_imported() {
    let (mut d, [hi, _, _, out]) = station();
    d.cables = vec![cable("C1", hi, out)];
    let s = session(d.clone());
    let (_, json) = s.export();
    let mut fresh = session(Document::new_default(&limits()));
    fresh.import(&json, 0.0).unwrap();
    assert_eq!(fresh.document().cables, d.cables);
}

#[test]
fn framing_a_cable_fits_its_path() {
    let (mut d, [hi, _, _, out]) = station();
    d.cables = vec![cable("C1", hi, out)];
    let id = ObjectId::Cable(d.cables[0].id);
    let mut s = session(d);
    s.frame(id, 0.0);
    let mut now = 0.0;
    settle(&mut s, &mut now);
    let path = cable_points(s.document(), &s.limits, &s.document().cables[0]).unwrap();
    for p in path {
        let q = px(&s, p);
        assert!(
            s.viewport().cmpge(q).all() && q.cmpge(Vec2::ZERO).all(),
            "{q}"
        );
    }
}

/// The station with cable "C1" from HI to OUT, coloured green.
fn wired() -> (Document, [PortId; 4]) {
    let (mut d, ids) = station();
    let [hi, _, _, out] = ids;
    let mut c = cable("C1", hi, out);
    c.color = Rgb { r: 0, g: 160, b: 0 };
    d.cables = vec![c];
    (d, ids)
}

/// Shift-presses port `end` and moves just far enough to start a drag.
fn pick_up(s: &mut Session, end: PortId) {
    let start = port_px(s, end);
    s.pointer_down_with_shift(start, Button::Left, true);
    s.pointer_move(start + Vec2::new(5.0, 0.0), false, 0.0);
}

#[test]
fn shift_dragging_a_connected_port_replugs_its_cable_end() {
    let (d, [hi, lo, _, out]) = wired();
    let original = d.cables[0].clone();
    let mut s = session(d);
    let to = port_px(&s, lo);
    connect_drag(&mut s, out, to);
    let c = &s.document().cables[0];
    assert_eq!((c.id, c.a, c.b), (original.id, hi, lo));
    assert_eq!((&c.name, c.color), (&original.name, original.color));
    assert_eq!(s.selection(), Some(ObjectId::Cable(c.id)));
    assert_eq!(s.document().cables.len(), 1);
    s.undo();
    assert_eq!(s.document().cables[0], original);
}

#[test]
fn a_picked_up_end_follows_the_pointer_in_the_cable_colour() {
    let (d, [hi, lo, sense, out]) = wired();
    let id = d.cables[0].id;
    let mut s = session(d);
    pick_up(&mut s, out);
    assert!(s.is_dragging(), "the trash zone is shown");

    // Over empty space: the cable hangs from HI to the pointer.
    s.pointer_move(Vec2::new(400.0, 590.0), false, 0.0);
    let preview = s.cable_preview().expect("loose end");
    assert!(preview.valid);
    assert_eq!(
        preview.from,
        port_anchor(s.document(), &s.limits, hi).unwrap()
    );
    assert_eq!(preview.color, Rgb { r: 0, g: 160, b: 0 });
    assert_eq!(
        s.scene().tubes.len(),
        CABLE_SEGMENTS,
        "only the preview is drawn"
    );

    // Over a free port: the re-plugged cable.
    s.pointer_move(port_px(&s, sense), false, 0.0);
    assert_eq!(s.cable_preview(), None);
    assert_eq!(
        s.shown_document().cable(id).map(|c| (c.a, c.b)),
        Some((hi, sense))
    );
    assert_eq!(s.hovered_port(), Some(sense));
    assert_eq!(s.scene().tubes.len(), CABLE_SEGMENTS);

    // Over its other end: red.
    s.pointer_move(port_px(&s, hi), false, 0.0);
    assert!(s.cable_preview().is_some_and(|p| !p.valid));
    assert_eq!(s.scene().tubes.len(), CABLE_SEGMENTS);

    // Back over the port it came from: drawn as it was.
    s.pointer_move(port_px(&s, out), false, 0.0);
    assert_eq!(s.cable_preview(), None);
    assert_eq!(s.shown_document(), s.document());
    assert_eq!(s.scene().tubes.len(), CABLE_SEGMENTS);

    s.pointer_move(port_px(&s, lo), false, 0.0);
    s.escape(0.0);
    assert_eq!(s.document().cables[0].b, out);
    assert_eq!(s.revision(), 0);
    assert_eq!(s.scene().tubes.len(), CABLE_SEGMENTS);
}

#[test]
fn replugging_onto_a_taken_port_or_empty_space_changes_nothing() {
    let (mut d, [hi, lo, sense, out]) = wired();
    d.cables.push(cable("C2", lo, sense));
    let mut s = session(d);
    pick_up(&mut s, out);
    s.pointer_move(port_px(&s, lo), false, 0.0);
    assert!(s.cable_preview().is_some_and(|p| !p.valid));
    s.pointer_up(port_px(&s, lo), false, 0.0);

    connect_drag(&mut s, out, Vec2::new(400.0, 590.0));
    let to = port_px(&s, out);
    connect_drag(&mut s, out, to);
    assert_eq!(s.revision(), 0, "no change and no empty undo step");
    assert_eq!(
        (s.document().cables[0].a, s.document().cables[0].b),
        (hi, out)
    );
}

#[test]
fn dropping_a_picked_up_end_in_the_trash_deletes_the_cable() {
    let (d, [_, _, _, out]) = wired();
    let mut s = session(d);
    pick_up(&mut s, out);
    let to = Vec2::new(400.0, 590.0);
    s.pointer_move(to, true, 0.0);
    assert_eq!(s.cable_preview(), None);
    s.pointer_up(to, true, 0.0);
    assert!(s.document().cables.is_empty());
    assert!(s.document().port(out).is_some(), "the ports stay");
}

#[test]
fn the_port_panel_handle_picks_up_the_cable_end() {
    let (d, [hi, _, sense, out]) = wired();
    let id = d.cables[0].id;
    let mut s = session(d);
    s.start_drag(DragSource::CableEnd { cable: id, end: hi }, 0.0);
    let to = port_px(&s, sense);
    s.pointer_move(to, false, 0.0);
    s.pointer_up(to, false, 0.0);
    let c = &s.document().cables[0];
    assert_eq!((c.a, c.b), (sense, out));
}

#[test]
fn a_cable_between_different_connector_types_is_previewed_amber_but_allowed() {
    let (mut d, [hi, lo, sense, _]) = station();
    let ports = &mut d.racks[0].devices[0].ports;
    ports[0].kind = rackwright_core::model::PortKind::Bnc;
    ports[1].kind = rackwright_core::model::PortKind::Sma;
    ports[2].kind = rackwright_core::model::PortKind::Bnc;
    let mut s = session(d);
    pick_up(&mut s, hi);

    s.pointer_move(port_px(&s, lo), false, 0.0);
    let preview = s.cable_preview().expect("amber preview");
    assert!(preview.valid);
    assert_eq!(preview.color, rackwright_render::scene::CABLE_WARNING);
    assert_eq!(
        preview.to,
        port_anchor(s.document(), &s.limits, lo).unwrap()
    );
    assert_eq!(
        s.shown_document().cables.len(),
        1,
        "the planned cable is in the preview"
    );
    let amber = rackwright_render::scene::rgba(rackwright_render::scene::CABLE_WARNING, 1.0);
    let cable_tubes: Vec<_> = s
        .scene()
        .tubes
        .into_iter()
        .filter(|t| t.radius == rackwright_render::layout::CABLE_RADIUS_MM)
        .collect();
    assert_eq!(cable_tubes.len(), CABLE_SEGMENTS, "drawn once");
    assert!(cable_tubes.iter().all(|t| t.color == amber), "in amber");

    // Same type: the ordinary preview.
    s.pointer_move(port_px(&s, sense), false, 0.0);
    assert_eq!(s.cable_preview(), None);

    s.pointer_move(port_px(&s, lo), false, 0.0);
    s.pointer_up(port_px(&s, lo), false, 0.0);
    let doc = s.document();
    assert!(doc.cable_mismatch(&doc.cables[0]).is_some());
}
