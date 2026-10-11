mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::ids::{DeviceId, PortId};
use rackwright_core::model::{Device, Document};
use rackwright_core::port_grid::{Cell, PushDir};

fn drop_port(
    d: &Document,
    device: DeviceId,
    source: PortSource,
    row: u32,
    col: u32,
    dir: PushDir,
) -> Result<Plan, Rejection> {
    plan_port_drop(d, &limits(), device, &source, Cell { row, col }, dir)
}

fn new(n: &str) -> PortSource {
    PortSource::New {
        name: name(n),
        kind: rackwright_core::model::PortKind::Unspecified,
        gender: Default::default(),
    }
}

/// `(name, row, col)` of every port, sorted by name.
fn cells(device: &Device) -> Vec<(String, u32, u32)> {
    let mut v: Vec<(String, u32, u32)> = device
        .ports
        .iter()
        .map(|p| (p.name.to_string(), p.row, p.col))
        .collect();
    v.sort();
    v
}

fn triples(v: &[(&str, u32, u32)]) -> Vec<(String, u32, u32)> {
    v.iter().map(|(n, r, c)| (n.to_string(), *r, *c)).collect()
}

fn single_device_doc(dev: Device) -> (Document, DeviceId) {
    let id = dev.id;
    (doc(vec![rack("R", 42, vec![dev])]), id)
}

fn first_device(plan: &Plan) -> &Device {
    &plan.document.racks[0].devices[0]
}

#[test]
fn new_port_lands_in_a_free_cell() {
    let (d, dev) = single_device_doc(device("D", 10, 1));
    let plan = drop_port(&d, dev, new("CH1"), 0, 2, PushDir::Right).unwrap();
    assert_eq!(cells(first_device(&plan)), triples(&[("CH1", 0, 2)]));
    assert_eq!(
        plan.subject,
        Some(ObjectId::Port(first_device(&plan).ports[0].id))
    );
}

#[test]
fn new_port_name_must_be_unique_on_its_device() {
    let (d, dev) = single_device_doc(with_ports(device("D", 10, 1), vec![port("CH1", 0, 0)]));
    assert_eq!(
        drop_port(&d, dev, new("ch1"), 0, 3, PushDir::Right).unwrap_err(),
        Rejection::NameTaken("ch1".into())
    );
}

#[test]
fn same_port_name_is_fine_on_another_device() {
    let other = with_ports(device("E", 20, 1), vec![port("CH1", 0, 0)]);
    let target = device("D", 10, 1);
    let dev = target.id;
    let d = doc(vec![rack("R", 42, vec![target, other])]);
    assert!(drop_port(&d, dev, new("CH1"), 0, 0, PushDir::Right).is_ok());
}

#[test]
fn new_port_pushes_the_occupant() {
    let (d, dev) = single_device_doc(with_ports(device("D", 10, 1), vec![port("A", 0, 1)]));
    let plan = drop_port(&d, dev, new("B"), 0, 1, PushDir::Right).unwrap();
    assert_eq!(
        cells(first_device(&plan)),
        triples(&[("A", 0, 2), ("B", 0, 1)])
    );
}

#[test]
fn new_port_pushes_vertically_on_a_taller_device() {
    let (d, dev) = single_device_doc(with_ports(device("D", 10, 2), vec![port("A", 0, 2)]));
    let plan = drop_port(&d, dev, new("B"), 0, 2, PushDir::Up).unwrap();
    assert_eq!(
        cells(first_device(&plan)),
        triples(&[("A", 1, 2), ("B", 0, 2)])
    );
}

#[test]
fn push_off_the_face_is_rejected() {
    let (d, dev) = single_device_doc(with_ports(
        device("D", 10, 1),
        vec![port("A", 0, 3), port("B", 0, 4)],
    ));
    assert_eq!(
        drop_port(&d, dev, new("C"), 0, 3, PushDir::Right).unwrap_err(),
        Rejection::NoRoomForPorts
    );
}

#[test]
fn target_outside_the_face_is_rejected() {
    let (d, dev) = single_device_doc(device("D", 10, 1));
    assert_eq!(
        drop_port(&d, dev, new("C"), 1, 0, PushDir::Right).unwrap_err(),
        Rejection::OutOfGrid
    );
    assert_eq!(
        drop_port(&d, dev, new("C"), 0, 5, PushDir::Right).unwrap_err(),
        Rejection::OutOfGrid
    );
}

#[test]
fn moving_a_port_onto_its_own_cell_changes_nothing() {
    let a = port("A", 0, 1);
    let a_id = a.id;
    let (d, dev) = single_device_doc(with_ports(device("D", 10, 1), vec![a, port("B", 0, 2)]));
    let plan = drop_port(&d, dev, PortSource::Existing(a_id), 0, 1, PushDir::Left).unwrap();
    assert_eq!(
        cells(first_device(&plan)),
        triples(&[("A", 0, 1), ("B", 0, 2)])
    );
}

#[test]
fn moving_a_port_can_push_into_the_cell_it_vacated() {
    let a = port("A", 0, 1);
    let a_id = a.id;
    let (d, dev) = single_device_doc(with_ports(device("D", 10, 1), vec![a, port("B", 0, 2)]));
    let plan = drop_port(&d, dev, PortSource::Existing(a_id), 0, 2, PushDir::Left).unwrap();
    assert_eq!(
        cells(first_device(&plan)),
        triples(&[("A", 0, 2), ("B", 0, 1)])
    );
}

#[test]
fn ports_cannot_move_between_devices() {
    let foreign = port("X", 0, 0);
    let foreign_id = foreign.id;
    let target = device("D", 10, 1);
    let dev = target.id;
    let d = doc(vec![rack(
        "R",
        42,
        vec![target, with_ports(device("E", 20, 1), vec![foreign])],
    )]);
    assert_eq!(
        drop_port(
            &d,
            dev,
            PortSource::Existing(foreign_id),
            0,
            0,
            PushDir::Right
        )
        .unwrap_err(),
        Rejection::WrongDevice
    );
}

#[test]
fn unknown_port_or_device_is_not_found() {
    let (d, dev) = single_device_doc(device("D", 10, 1));
    assert_eq!(
        drop_port(
            &d,
            dev,
            PortSource::Existing(PortId::new()),
            0,
            0,
            PushDir::Right
        )
        .unwrap_err(),
        Rejection::NotFound
    );
    assert_eq!(
        drop_port(&d, DeviceId::new(), new("A"), 0, 0, PushDir::Right).unwrap_err(),
        Rejection::NotFound
    );
    assert_eq!(
        plan_remove_port(&d, PortId::new()).unwrap_err(),
        Rejection::NotFound
    );
}

#[test]
fn remove_port() {
    let a = port("A", 0, 1);
    let a_id = a.id;
    let (d, _) = single_device_doc(with_ports(device("D", 10, 1), vec![a, port("B", 0, 2)]));
    let plan = plan_remove_port(&d, a_id).unwrap();
    assert_eq!(cells(first_device(&plan)), triples(&[("B", 0, 2)]));
}

#[test]
fn rename_port_rejects_clash_on_same_device() {
    let a = port("A", 0, 1);
    let a_id = a.id;
    let (d, _) = single_device_doc(with_ports(device("D", 10, 1), vec![a, port("B", 0, 2)]));
    assert_eq!(
        plan_rename_port(&d, a_id, name("b")).unwrap_err(),
        Rejection::NameTaken("b".into())
    );
    let plan = plan_rename_port(&d, a_id, name("CH9")).unwrap();
    assert_eq!(
        cells(first_device(&plan)),
        triples(&[("B", 0, 2), ("CH9", 0, 1)])
    );
}
