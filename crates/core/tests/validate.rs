mod common;

use common::*;
use rackwright_core::model::Document;
use rackwright_core::validate::{ValidationError, validate};

fn check(d: &Document) -> Result<(), ValidationError> {
    validate(d, &limits())
}

fn s(v: &str) -> String {
    v.to_string()
}

#[test]
fn default_and_populated_documents_are_valid() {
    assert_eq!(check(&Document::new_default(&limits())), Ok(()));
    let d = doc(vec![rack(
        "R",
        42,
        vec![
            with_ports(device("A", 1, 2), vec![port("P1", 0, 0), port("P2", 1, 4)]),
            device("B", 3, 1),
        ],
    )]);
    assert_eq!(check(&d), Ok(()));
}

#[test]
fn duplicate_ids_are_rejected() {
    let a = device("A", 1, 1);
    let mut b = device("B", 1, 1);
    b.id = a.id;
    let d = doc(vec![rack("R1", 42, vec![a]), rack("R2", 42, vec![b])]);
    assert!(matches!(check(&d), Err(ValidationError::DuplicateId(_))));
}

#[test]
fn rack_names_must_be_unique_case_insensitively() {
    let d = doc(vec![rack("Rack1", 42, vec![]), rack("rack1", 42, vec![])]);
    assert_eq!(
        check(&d),
        Err(ValidationError::DuplicateRackName(s("rack1")))
    );
}

#[test]
fn rack_height_must_be_positive() {
    let d = doc(vec![rack("R", 0, vec![])]);
    assert_eq!(
        check(&d),
        Err(ValidationError::RackZeroHeight { rack: s("R") })
    );
}

#[test]
fn device_names_must_be_unique_within_a_rack() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![device("DMM", 1, 1), device("dmm", 5, 1)],
    )]);
    assert_eq!(
        check(&d),
        Err(ValidationError::DuplicateDeviceName {
            rack: s("R"),
            name: s("dmm")
        })
    );
}

#[test]
fn device_height_must_be_positive() {
    let d = doc(vec![rack("R", 42, vec![device("A", 1, 0)])]);
    assert_eq!(
        check(&d),
        Err(ValidationError::DeviceZeroHeight {
            rack: s("R"),
            device: s("A")
        })
    );
}

#[test]
fn devices_must_lie_inside_the_rack_without_overflow() {
    let outside = ValidationError::DeviceOutsideRack {
        rack: s("R"),
        device: s("A"),
    };
    for (bottom, height) in [(0, 1), (42, 2), (u32::MAX, 2), (1, u32::MAX)] {
        let d = doc(vec![rack("R", 42, vec![device("A", bottom, height)])]);
        assert_eq!(
            check(&d),
            Err(outside.clone()),
            "bottom {bottom} height {height}"
        );
    }
}

#[test]
fn overlapping_devices_are_reported_with_names_and_position() {
    let d = doc(vec![rack(
        "Rack1",
        42,
        vec![device("B", 2, 1), device("A", 1, 2)],
    )]);
    let err = check(&d).unwrap_err();
    assert_eq!(
        err,
        ValidationError::DevicesOverlap {
            rack: s("Rack1"),
            a: s("A"),
            b: s("B"),
            u: 2
        }
    );
    assert_eq!(
        err.to_string(),
        "Rack 'Rack1': devices 'A' and 'B' overlap at U2"
    );
}

#[test]
fn port_names_must_be_unique_within_a_device() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![with_ports(
            device("D", 1, 1),
            vec![port("CH1", 0, 0), port("ch1", 0, 1)],
        )],
    )]);
    assert_eq!(
        check(&d),
        Err(ValidationError::DuplicatePortName {
            device: s("D"),
            name: s("ch1")
        })
    );
}

#[test]
fn ports_must_lie_inside_the_grid() {
    for (row, col) in [(1, 0), (0, 5), (u32::MAX, 0)] {
        let d = doc(vec![rack(
            "R",
            42,
            vec![with_ports(device("D", 1, 1), vec![port("P", row, col)])],
        )]);
        assert_eq!(
            check(&d),
            Err(ValidationError::PortOutsideGrid {
                device: s("D"),
                port: s("P")
            }),
            "row {row} col {col}"
        );
    }
}

#[test]
fn ports_must_not_share_a_cell() {
    let d = doc(vec![rack(
        "R",
        42,
        vec![with_ports(
            device("D", 1, 1),
            vec![port("A", 0, 2), port("B", 0, 2)],
        )],
    )]);
    assert_eq!(
        check(&d),
        Err(ValidationError::PortsOverlap {
            device: s("D"),
            a: s("A"),
            b: s("B")
        })
    );
}

#[test]
fn rack_height_must_not_exceed_the_limit() {
    assert_eq!(check(&doc(vec![rack("R", 100, vec![])])), Ok(()));
    let d = doc(vec![rack("R", u32::MAX, vec![device("A", u32::MAX, 1)])]);
    assert_eq!(
        check(&d),
        Err(ValidationError::RackTooTall {
            rack: s("R"),
            max: 100
        })
    );
}

fn two_ported_devices() -> Document {
    doc(vec![rack(
        "R",
        42,
        vec![
            with_ports(device("A", 1, 1), vec![port("P1", 0, 0), port("P2", 0, 1)]),
            with_ports(device("B", 2, 1), vec![port("P1", 0, 0)]),
        ],
    )])
}

fn port_ids(
    d: &Document,
) -> (
    rackwright_core::ids::PortId,
    rackwright_core::ids::PortId,
    rackwright_core::ids::PortId,
) {
    let a = &d.racks[0].devices[0].ports;
    let b = &d.racks[0].devices[1].ports;
    (a[0].id, a[1].id, b[0].id)
}

#[test]
fn documents_with_cables_are_valid() {
    let mut d = two_ported_devices();
    let (a1, a2, b1) = port_ids(&d);
    d.cables = vec![cable("C1", a1, b1)];
    assert_eq!(check(&d), Ok(()));
    d.cables = vec![cable("C1", a1, a2)];
    assert_eq!(check(&d), Ok(()));
}

#[test]
fn cable_ids_must_be_unique_among_all_ids() {
    let mut d = two_ported_devices();
    let (a1, _, b1) = port_ids(&d);
    let mut c = cable("C1", a1, b1);
    c.id = rackwright_core::ids::CableId(d.racks[0].id.0);
    d.cables = vec![c];
    assert!(matches!(check(&d), Err(ValidationError::DuplicateId(_))));
}

#[test]
fn cable_names_must_be_unique() {
    let mut d = two_ported_devices();
    let (a1, a2, b1) = port_ids(&d);
    d.cables = vec![cable("Lead", a1, b1), cable("LEAD", a2, a1)];
    // The name check comes before the per-port check.
    assert_eq!(
        check(&d),
        Err(ValidationError::DuplicateCableName(s("LEAD")))
    );
    d.cables[1].a = a2;
    d.cables[1].b = rackwright_core::ids::PortId::new();
    d.cables[1].name = name("Other");
    assert_eq!(
        check(&d),
        Err(ValidationError::CableEndMissing { cable: s("Other") })
    );
}

#[test]
fn cable_ends_must_differ() {
    let mut d = two_ported_devices();
    let (a1, _, _) = port_ids(&d);
    d.cables = vec![cable("Loop", a1, a1)];
    assert_eq!(
        check(&d),
        Err(ValidationError::CableEndsEqual { cable: s("Loop") })
    );
}

#[test]
fn a_port_takes_at_most_one_cable() {
    let mut d = two_ported_devices();
    let (a1, a2, b1) = port_ids(&d);
    d.cables = vec![cable("C1", a1, b1), cable("C2", a2, b1)];
    let err = check(&d).unwrap_err();
    assert_eq!(
        err,
        ValidationError::PortHasTwoCables {
            device: s("B"),
            port: s("P1"),
            a: s("C1"),
            b: s("C2"),
        }
    );
    assert_eq!(
        err.to_string(),
        "Device 'B': port 'P1' has two cables, 'C1' and 'C2'"
    );
}
