mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::file_format::{LoadError, from_json, to_json};
use rackwright_core::model::{Document, PortKind};
use rackwright_core::port_grid::{Cell, PushDir};

#[test]
fn every_kind_has_a_unique_key_and_label_that_round_trip() {
    assert_eq!(PortKind::ALL.len(), 11);
    assert_eq!(PortKind::ALL[0], PortKind::Unspecified);
    for kind in PortKind::ALL {
        assert_eq!(PortKind::from_key(kind.key()), Some(kind));
        assert!(!kind.label().is_empty());
    }
    let mut keys: Vec<&str> = PortKind::ALL.iter().map(|k| k.key()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), PortKind::ALL.len());
    assert_eq!(PortKind::from_key("hdmi"), None);
    assert_eq!(PortKind::Lan.label(), "LAN (RJ45)");
    assert_eq!(PortKind::NType.key(), "n_type");
}

#[test]
fn only_real_connector_types_are_specific() {
    assert!(!PortKind::Unspecified.is_specific());
    assert!(!PortKind::Other.is_specific());
    assert!(PortKind::Bnc.is_specific());
}

/// DMM with HI and LO, PSU with OUT; cable C1 from HI to OUT.
fn wired(hi: PortKind, out: PortKind) -> Document {
    let mut dmm = with_ports(
        device("DMM", 10, 1),
        vec![port("HI", 0, 0), port("LO", 0, 1)],
    );
    let mut psu = with_ports(device("PSU", 1, 1), vec![port("OUT", 0, 0)]);
    dmm.ports[0].kind = hi;
    psu.ports[0].kind = out;
    let c = cable("C1", dmm.ports[0].id, psu.ports[0].id);
    let mut d = doc(vec![rack("R", 42, vec![dmm, psu])]);
    d.cables = vec![c];
    d
}

#[test]
fn cables_between_different_specific_types_mismatch() {
    use PortKind::*;
    let mismatch = |a, b| {
        let d = wired(a, b);
        d.cable_mismatch(&d.cables[0])
    };
    assert_eq!(mismatch(Bnc, Sma), Some((Bnc, Sma)));
    assert_eq!(mismatch(Bnc, Bnc), None);
    assert_eq!(mismatch(Unspecified, Sma), None);
    assert_eq!(mismatch(Bnc, Other), None);
    assert_eq!(mismatch(Other, Other), None);
}

#[test]
fn the_type_of_a_port_can_be_changed_even_when_its_cable_then_mismatches() {
    let d = wired(PortKind::Bnc, PortKind::Bnc);
    let out = d.cables[0].b;
    let plan = plan_set_port_kind(&d, out, PortKind::Sma).unwrap();
    assert_eq!(plan.document.port(out).unwrap().2.kind, PortKind::Sma);
    assert_eq!(plan.subject, Some(ObjectId::Port(out)));
    assert_eq!(plan.document.cables, d.cables);
    assert!(
        plan.document
            .cable_mismatch(&plan.document.cables[0])
            .is_some()
    );
    assert_eq!(
        plan_set_port_kind(&d, rackwright_core::ids::PortId::new(), PortKind::Sma),
        Err(Rejection::NotFound)
    );
}

#[test]
fn new_ports_get_the_chosen_type() {
    let dev = device("D", 1, 1);
    let id = dev.id;
    let d = doc(vec![rack("R", 42, vec![dev])]);
    let source = PortSource::New {
        name: name("CH1"),
        kind: PortKind::Usb,
        gender: Default::default(),
    };
    let plan = plan_port_drop(
        &d,
        &limits(),
        id,
        &source,
        Cell { row: 0, col: 2 },
        PushDir::Right,
    )
    .unwrap();
    assert_eq!(
        plan.document.racks[0].devices[0].ports[0].kind,
        PortKind::Usb
    );
}

#[test]
fn every_type_survives_a_file_round_trip() {
    let ports = PortKind::ALL
        .iter()
        .enumerate()
        .map(|(i, &kind)| {
            let mut p = port(&format!("P{i}"), i as u32 / 5, i as u32 % 5);
            p.kind = kind;
            p
        })
        .collect();
    let d = doc(vec![rack(
        "R",
        42,
        vec![with_ports(device("D", 1, 3), ports)],
    )]);
    let json = to_json(&d);
    assert!(json.starts_with("{\n  \"format_version\": 5,"), "{json}");
    assert!(json.contains("\"kind\": \"n_type\""), "{json}");
    assert_eq!(from_json(&json, &limits()), Ok(d));
}

#[test]
fn version_2_files_still_load_and_unknown_types_are_malformed() {
    let d = wired(PortKind::Unspecified, PortKind::Unspecified);
    let v2 = to_json(&d).replacen("\"format_version\": 5", "\"format_version\": 2", 1);
    assert_eq!(from_json(&v2, &limits()), Ok(d.clone()));
    let bad = to_json(&d).replacen("\"kind\": \"unspecified\"", "\"kind\": \"hdmi\"", 1);
    assert!(matches!(
        from_json(&bad, &limits()),
        Err(LoadError::Malformed(_))
    ));
}
