mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::ids::{DeviceId, RackId};
use rackwright_core::model::{Document, Rgb};

fn drop_new(
    d: &Document,
    rack: RackId,
    n: &str,
    height_u: u32,
    bottom_u: u32,
) -> Result<Plan, Rejection> {
    let source = DeviceSource::New {
        name: name(n),
        height_u,
    };
    plan_device_drop(d, &limits(), &source, rack, bottom_u)
}

fn drop_existing(
    d: &Document,
    id: DeviceId,
    rack: RackId,
    bottom_u: u32,
) -> Result<Plan, Rejection> {
    plan_device_drop(d, &limits(), &DeviceSource::Existing(id), rack, bottom_u)
}

fn pairs(v: &[(&str, u32)]) -> Vec<(String, u32)> {
    v.iter().map(|(n, b)| (n.to_string(), *b)).collect()
}

#[test]
fn new_device_lands_in_a_free_slot() {
    let d = doc(vec![rack("R", 42, vec![])]);
    let plan = drop_new(&d, d.racks[0].id, "DMM", 2, 10).unwrap();
    let r = &plan.document.racks[0];
    assert_eq!(layout(r), pairs(&[("DMM", 10)]));
    let dmm = device_named(r, "DMM");
    assert_eq!((dmm.height_u, dmm.color), (2, Rgb::NEUTRAL_GREY));
    assert_eq!(plan.subject, Some(ObjectId::Device(dmm.id)));
}

#[test]
fn new_device_is_clamped_to_the_rack_top() {
    let d = doc(vec![rack("R", 42, vec![])]);
    let plan = drop_new(&d, d.racks[0].id, "DMM", 2, 42).unwrap();
    assert_eq!(layout(&plan.document.racks[0]), pairs(&[("DMM", 41)]));
}

#[test]
fn drop_pushes_neighbours_away_from_its_centre() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![device("A", 10, 1), device("B", 12, 1)],
    )]);
    let plan = drop_new(&d, d.racks[0].id, "N", 3, 10).unwrap();
    assert_eq!(
        layout(&plan.document.racks[0]),
        pairs(&[("A", 9), ("N", 10), ("B", 13)])
    );
}

#[test]
fn drop_without_room_is_rejected() {
    let d = doc(vec![rack("R", 42, vec![device("A", 1, 1)])]);
    assert_eq!(
        drop_new(&d, d.racks[0].id, "N", 1, 1).unwrap_err(),
        Rejection::NoRoomBelow
    );
}

#[test]
fn new_device_name_clash_is_auto_renamed() {
    let d = doc(vec![rack("R", 42, vec![device("DMM", 1, 1)])]);
    let plan = drop_new(&d, d.racks[0].id, "dmm", 1, 5).unwrap();
    assert_eq!(
        layout(&plan.document.racks[0]),
        pairs(&[("DMM", 1), ("dmm_2", 5)])
    );
}

#[test]
fn new_device_height_is_checked() {
    let d = doc(vec![rack("R", 4, vec![])]);
    assert_eq!(
        drop_new(&d, d.racks[0].id, "N", 0, 1).unwrap_err(),
        Rejection::ZeroHeight
    );
    assert_eq!(
        drop_new(&d, d.racks[0].id, "N", 5, 1).unwrap_err(),
        Rejection::TooTall
    );
}

#[test]
fn dropping_a_device_on_its_own_position_changes_nothing() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![device("A", 10, 2), device("B", 12, 1)],
    )]);
    let a = device_named(&d.racks[0], "A").id;
    let plan = drop_existing(&d, a, d.racks[0].id, 10).unwrap();
    assert_eq!(
        layout(&plan.document.racks[0]),
        pairs(&[("A", 10), ("B", 12)])
    );
}

#[test]
fn moving_within_a_rack_ignores_its_own_old_position() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![device("A", 10, 2), device("B", 12, 1)],
    )]);
    let a = device_named(&d.racks[0], "A").id;
    let plan = drop_existing(&d, a, d.racks[0].id, 11).unwrap();
    assert_eq!(
        layout(&plan.document.racks[0]),
        pairs(&[("A", 11), ("B", 13)])
    );
}

#[test]
fn moving_between_racks_auto_renames_and_keeps_identity_and_ports() {
    let moving = with_ports(device("DMM", 3, 1), vec![port("CH1", 0, 0)]);
    let moving_id = moving.id;
    let d = doc(vec![
        rack("R1", 42, vec![moving]),
        rack("R2", 42, vec![device("DMM", 1, 1)]),
    ]);
    let plan = drop_existing(&d, moving_id, d.racks[1].id, 5).unwrap();
    assert!(rack_named(&plan.document, "R1").devices.is_empty());
    let r2 = rack_named(&plan.document, "R2");
    assert_eq!(layout(r2), pairs(&[("DMM", 1), ("DMM_2", 5)]));
    let moved = device_named(r2, "DMM_2");
    assert_eq!(moved.id, moving_id);
    assert_eq!(moved.ports[0].name.as_str(), "CH1");
}

#[test]
fn unknown_device_or_rack_is_not_found() {
    let d = doc(vec![rack("R", 42, vec![device("A", 1, 1)])]);
    let a = d.racks[0].devices[0].id;
    assert_eq!(
        drop_existing(&d, DeviceId::new(), d.racks[0].id, 1).unwrap_err(),
        Rejection::NotFound
    );
    assert_eq!(
        drop_existing(&d, a, RackId::new(), 1).unwrap_err(),
        Rejection::NotFound
    );
    assert_eq!(
        plan_remove_device(&d, DeviceId::new()).unwrap_err(),
        Rejection::NotFound
    );
}

#[test]
fn remove_device() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![device("A", 1, 1), device("B", 2, 1)],
    )]);
    let plan = plan_remove_device(&d, d.racks[0].devices[0].id).unwrap();
    assert_eq!(layout(&plan.document.racks[0]), pairs(&[("B", 2)]));
}

#[test]
fn rename_device_is_unique_per_rack_only() {
    let d = doc(vec![
        rack("R1", 42, vec![device("A", 1, 1), device("B", 2, 1)]),
        rack("R2", 42, vec![device("C", 1, 1)]),
    ]);
    let a = device_named(&d.racks[0], "A").id;
    assert_eq!(
        plan_rename_device(&d, a, name("b")).unwrap_err(),
        Rejection::NameTaken("b".into())
    );
    let plan = plan_rename_device(&d, a, name("C")).unwrap();
    assert_eq!(
        layout(&plan.document.racks[0]),
        pairs(&[("C", 1), ("B", 2)])
    );
}

#[test]
fn set_device_color() {
    let d = doc(vec![rack("R", 42, vec![device("A", 1, 1)])]);
    let red = Rgb { r: 255, g: 0, b: 0 };
    let plan = plan_set_device_color(&d, d.racks[0].devices[0].id, red).unwrap();
    assert_eq!(plan.document.racks[0].devices[0].color, red);
}

#[test]
fn growing_a_device_pushes_devices_above_it() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![device("A", 10, 1), device("B", 11, 1), device("C", 5, 1)],
    )]);
    let a = device_named(&d.racks[0], "A").id;
    let plan = plan_set_device_height(&d, &limits(), a, 2).unwrap();
    let r = &plan.document.racks[0];
    assert_eq!(layout(r), pairs(&[("C", 5), ("A", 10), ("B", 12)]));
    assert_eq!(device_named(r, "A").height_u, 2);
}

#[test]
fn growing_past_the_rack_top_is_rejected() {
    let d = doc(vec![rack("R", 42, vec![device("A", 42, 1)])]);
    let a = d.racks[0].devices[0].id;
    assert_eq!(
        plan_set_device_height(&d, &limits(), a, 2).unwrap_err(),
        Rejection::NoRoomAbove
    );
}

#[test]
fn growing_that_pushes_a_neighbour_out_is_rejected() {
    let d = doc(vec![rack(
        "R",
        12,
        vec![device("A", 10, 1), device("B", 11, 2)],
    )]);
    let a = device_named(&d.racks[0], "A").id;
    assert_eq!(
        plan_set_device_height(&d, &limits(), a, 2).unwrap_err(),
        Rejection::NoRoomAbove
    );
}

#[test]
fn shrinking_is_rejected_when_ports_would_fall_off() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![
            with_ports(device("A", 1, 2), vec![port("TOP", 1, 0)]),
            with_ports(device("B", 5, 2), vec![port("LOW", 0, 0)]),
        ],
    )]);
    let a = device_named(&d.racks[0], "A").id;
    let b = device_named(&d.racks[0], "B").id;
    assert_eq!(
        plan_set_device_height(&d, &limits(), a, 1).unwrap_err(),
        Rejection::PortsOutside
    );
    let plan = plan_set_device_height(&d, &limits(), b, 1).unwrap();
    assert_eq!(device_named(&plan.document.racks[0], "B").height_u, 1);
}

#[test]
fn zero_height_is_rejected() {
    let d = doc(vec![rack("R", 42, vec![device("A", 1, 1)])]);
    let a = d.racks[0].devices[0].id;
    assert_eq!(
        plan_set_device_height(&d, &limits(), a, 0).unwrap_err(),
        Rejection::ZeroHeight
    );
}
