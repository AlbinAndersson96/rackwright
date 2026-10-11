mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::ids::PortId;
use rackwright_core::model::{Document, Rgb};

/// Two racks: R1 holds DMM (HI, LO) and PSU (OUT); R2 holds SCOPE (CH1).
fn station() -> Document {
    doc(vec![
        rack(
            "R1",
            42,
            vec![
                with_ports(
                    device("DMM", 10, 1),
                    vec![port("HI", 0, 0), port("LO", 0, 1)],
                ),
                with_ports(device("PSU", 1, 2), vec![port("OUT", 0, 0)]),
            ],
        ),
        rack(
            "R2",
            42,
            vec![with_ports(device("SCOPE", 5, 2), vec![port("CH1", 1, 2)])],
        ),
    ])
}

fn port_id(d: &Document, device: &str, port: &str) -> PortId {
    d.racks
        .iter()
        .flat_map(|r| &r.devices)
        .find(|dev| dev.name.as_str() == device)
        .and_then(|dev| dev.ports.iter().find(|p| p.name.as_str() == port))
        .unwrap_or_else(|| panic!("no port {device}.{port}"))
        .id
}

fn connect(d: &Document, a: PortId, b: PortId) -> Result<Plan, Rejection> {
    plan_connect(d, &limits(), a, b)
}

fn connected(d: &Document, a: (&str, &str), b: (&str, &str)) -> Document {
    connect(d, port_id(d, a.0, a.1), port_id(d, b.0, b.1))
        .unwrap()
        .document
}

#[test]
fn connecting_two_ports_adds_a_named_cable_and_selects_it() {
    let d = station();
    let (hi, out) = (port_id(&d, "DMM", "HI"), port_id(&d, "PSU", "OUT"));
    let plan = connect(&d, hi, out).unwrap();
    assert_eq!(plan.document.cables.len(), 1);
    let c = &plan.document.cables[0];
    assert_eq!((c.a, c.b), (hi, out));
    assert_eq!(c.name.as_str(), "Cable1");
    assert_eq!(c.color, Rgb::CABLE_BLUE);
    assert_eq!(plan.subject, Some(ObjectId::Cable(c.id)));
    assert!(d.cables.is_empty(), "the input is untouched");
}

#[test]
fn cables_may_join_racks_and_ports_of_one_device() {
    let d = station();
    let d = connected(&d, ("DMM", "HI"), ("SCOPE", "CH1"));
    let d = connected(&d, ("DMM", "LO"), ("PSU", "OUT"));
    assert_eq!(d.cables.len(), 2);

    let same_device = station();
    assert!(
        connect(
            &same_device,
            port_id(&same_device, "DMM", "HI"),
            port_id(&same_device, "DMM", "LO")
        )
        .is_ok()
    );
}

#[test]
fn new_cables_take_the_first_free_name() {
    let d = station();
    let d = connected(&d, ("DMM", "HI"), ("SCOPE", "CH1"));
    let d = connected(&d, ("DMM", "LO"), ("PSU", "OUT"));
    let names: Vec<&str> = d.cables.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Cable1", "Cable2"]);

    let first = d.cables[0].id;
    let d = plan_remove_cable(&d, first).unwrap().document;
    let d = plan_rename_cable(&d, d.cables[0].id, name("cable1"))
        .unwrap()
        .document;
    // "Cable1" is taken (case-insensitively), "Cable2" is free again.
    let d = connected(&d, ("DMM", "HI"), ("SCOPE", "CH1"));
    assert_eq!(d.cables[1].name.as_str(), "Cable2");
}

#[test]
fn a_cable_needs_two_different_existing_ports() {
    let d = station();
    let hi = port_id(&d, "DMM", "HI");
    assert_eq!(connect(&d, hi, hi), Err(Rejection::SamePort));
    assert_eq!(connect(&d, hi, PortId::new()), Err(Rejection::NotFound));
    assert_eq!(connect(&d, PortId::new(), hi), Err(Rejection::NotFound));
}

#[test]
fn a_port_takes_only_one_cable() {
    let d = station();
    let d = connected(&d, ("DMM", "HI"), ("PSU", "OUT"));
    let (hi, out, lo) = (
        port_id(&d, "DMM", "HI"),
        port_id(&d, "PSU", "OUT"),
        port_id(&d, "DMM", "LO"),
    );
    assert_eq!(connect(&d, lo, hi), Err(Rejection::PortInUse("HI".into())));
    assert_eq!(
        connect(&d, out, lo),
        Err(Rejection::PortInUse("OUT".into()))
    );
    assert_eq!(
        Rejection::PortInUse("HI".into()).to_string(),
        "port 'HI' already has a cable"
    );
}

#[test]
fn cables_can_be_found_from_their_ports() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let cable = d.cables[0].id;
    assert_eq!(
        d.cable_at_port(port_id(&d, "PSU", "OUT")).map(|c| c.id),
        Some(cable)
    );
    assert_eq!(d.cable_at_port(port_id(&d, "DMM", "LO")), None);
    assert_eq!(d.cable(cable).map(|c| c.name.as_str()), Some("Cable1"));
}

#[test]
fn removing_a_cable_leaves_its_ports() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let plan = plan_remove_cable(&d, d.cables[0].id).unwrap();
    assert!(plan.document.cables.is_empty());
    assert_eq!(plan.document.racks, d.racks);
    assert_eq!(plan.subject, None);
    assert_eq!(
        plan_remove_cable(&plan.document, d.cables[0].id),
        Err(Rejection::NotFound)
    );
}

#[test]
fn cable_names_are_unique_in_the_document() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let d = connected(&d, ("DMM", "LO"), ("SCOPE", "CH1"));
    let (first, second) = (d.cables[0].id, d.cables[1].id);
    assert_eq!(
        plan_rename_cable(&d, second, name("CABLE1")),
        Err(Rejection::NameTaken("CABLE1".into()))
    );
    let plan = plan_rename_cable(&d, first, name("Sense+")).unwrap();
    assert_eq!(plan.document.cables[0].name.as_str(), "Sense+");
    assert_eq!(plan.subject, Some(ObjectId::Cable(first)));
    // Renaming to its own name in another case is allowed.
    assert!(plan_rename_cable(&d, first, name("CABLE1")).is_ok());
}

#[test]
fn cables_can_be_recoloured() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let red = Rgb { r: 255, g: 0, b: 0 };
    let plan = plan_set_cable_color(&d, d.cables[0].id, red).unwrap();
    assert_eq!(plan.document.cables[0].color, red);
    assert_eq!(
        plan_set_cable_color(&d, rackwright_core::ids::CableId::new(), red),
        Err(Rejection::NotFound)
    );
}

#[test]
fn removing_a_port_removes_its_cable() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let d = connected(&d, ("DMM", "LO"), ("SCOPE", "CH1"));
    let plan = plan_remove_port(&d, port_id(&d, "PSU", "OUT")).unwrap();
    let names: Vec<&str> = plan
        .document
        .cables
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Cable2"]);
}

#[test]
fn removing_a_device_or_rack_removes_its_cables() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let d = connected(&d, ("DMM", "LO"), ("SCOPE", "CH1"));

    let dmm = d.racks[0].devices[0].id;
    assert!(
        plan_remove_device(&d, dmm)
            .unwrap()
            .document
            .cables
            .is_empty()
    );

    let r2 = d.racks[1].id;
    let names: Vec<String> = plan_remove_rack(&d, r2)
        .unwrap()
        .document
        .cables
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(names, ["Cable1"]);
}

#[test]
fn moving_ports_and_devices_keeps_cables() {
    let d = connected(&station(), ("DMM", "HI"), ("SCOPE", "CH1"));
    let dmm = d.racks[0].devices[0].id;
    let r2 = d.racks[1].id;
    let moved = plan_device_drop(&d, &limits(), &DeviceSource::Existing(dmm), r2, 20)
        .unwrap()
        .document;
    assert_eq!(moved.cables, d.cables);

    let hi = port_id(&d, "DMM", "HI");
    let moved = plan_port_drop(
        &d,
        &limits(),
        dmm,
        &PortSource::Existing(hi),
        rackwright_core::port_grid::Cell { row: 0, col: 4 },
        rackwright_core::port_grid::PushDir::Right,
    )
    .unwrap()
    .document;
    assert_eq!(moved.cables, d.cables);
}

#[test]
fn a_new_document_has_no_cables() {
    assert!(plan_new_document(&limits()).document.cables.is_empty());
}

fn replug(d: &Document, end: PortId, to: PortId) -> Result<Plan, Rejection> {
    plan_replug(d, d.cables[0].id, end, to)
}

#[test]
fn replugging_moves_one_end_and_keeps_the_cable() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let (hi, out, ch1) = (
        port_id(&d, "DMM", "HI"),
        port_id(&d, "PSU", "OUT"),
        port_id(&d, "SCOPE", "CH1"),
    );
    let plan = replug(&d, out, ch1).unwrap();
    let c = &plan.document.cables[0];
    assert_eq!((c.a, c.b), (hi, ch1));
    assert_eq!(
        (c.id, &c.name, c.color),
        (d.cables[0].id, &d.cables[0].name, d.cables[0].color)
    );
    assert_eq!(plan.subject, Some(ObjectId::Cable(c.id)));
    assert_eq!(plan.document.racks, d.racks);

    // The `a` end can be moved too.
    let plan = replug(&d, hi, ch1).unwrap();
    let c = &plan.document.cables[0];
    assert_eq!((c.a, c.b), (ch1, out));
}

#[test]
fn replugging_onto_the_same_port_changes_nothing() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let out = port_id(&d, "PSU", "OUT");
    assert_eq!(replug(&d, out, out).unwrap().document, d);
}

#[test]
fn replugging_follows_the_cable_rules() {
    let d = connected(&station(), ("DMM", "HI"), ("PSU", "OUT"));
    let d = connected(&d, ("DMM", "LO"), ("SCOPE", "CH1"));
    let (hi, out, lo) = (
        port_id(&d, "DMM", "HI"),
        port_id(&d, "PSU", "OUT"),
        port_id(&d, "DMM", "LO"),
    );
    assert_eq!(replug(&d, out, hi), Err(Rejection::SamePort));
    assert_eq!(replug(&d, out, lo), Err(Rejection::PortInUse("LO".into())));
    assert_eq!(replug(&d, out, PortId::new()), Err(Rejection::NotFound));
    // `end` must be one of the cable's ends.
    assert_eq!(replug(&d, lo, out), Err(Rejection::NotFound));
    assert_eq!(
        plan_replug(&d, rackwright_core::ids::CableId::new(), out, hi),
        Err(Rejection::NotFound)
    );
}
