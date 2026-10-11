mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::file_format::{from_json, to_json};
use rackwright_core::model::{Document, Gender, PortKind};
use rackwright_core::name::ModelText;
use rackwright_core::port_grid::{Cell, PushDir};

#[test]
fn genders_have_unique_keys_and_labels() {
    assert_eq!(Gender::ALL[0], Gender::Unspecified);
    assert_eq!(Gender::default(), Gender::Unspecified);
    for g in Gender::ALL {
        assert_eq!(Gender::from_key(g.key()), Some(g));
    }
    assert_eq!(Gender::Male.label(), "Male");
    assert_eq!(Gender::Other.label(), "Other (genderless)");
    assert_eq!(Gender::from_key("x"), None);
}

fn one_port() -> (Document, rackwright_core::ids::PortId) {
    let p = port("CH1", 0, 0);
    let id = p.id;
    (
        doc(vec![rack(
            "R",
            42,
            vec![with_ports(device("D", 1, 1), vec![p])],
        )]),
        id,
    )
}

#[test]
fn a_port_gender_can_be_set_and_new_ports_take_one() {
    let (d, id) = one_port();
    let plan = plan_set_port_gender(&d, id, Gender::Female).unwrap();
    assert_eq!(plan.document.port(id).unwrap().2.gender, Gender::Female);
    assert_eq!(plan.subject, Some(ObjectId::Port(id)));
    assert_eq!(
        plan_set_port_gender(&d, rackwright_core::ids::PortId::new(), Gender::Male),
        Err(Rejection::NotFound)
    );

    let device = d.racks[0].devices[0].id;
    let source = PortSource::New {
        name: name("CH2"),
        kind: PortKind::Bnc,
        gender: Gender::Male,
    };
    let plan = plan_port_drop(
        &d,
        &limits(),
        device,
        &source,
        Cell { row: 0, col: 3 },
        PushDir::Right,
    )
    .unwrap();
    let p = plan.document.racks[0].devices[0]
        .ports
        .iter()
        .find(|p| p.name.as_str() == "CH2")
        .unwrap();
    assert_eq!((p.kind, p.gender), (PortKind::Bnc, Gender::Male));
}

#[test]
fn gender_never_blocks_or_flags_a_cable() {
    let (mut d, a) = one_port();
    let mut b = port("CH2", 0, 1);
    b.kind = PortKind::Bnc;
    b.gender = Gender::Male;
    let bid = b.id;
    d.racks[0].devices[0].ports[0].kind = PortKind::Bnc;
    d.racks[0].devices[0].ports[0].gender = Gender::Male;
    d.racks[0].devices[0].ports.push(b);
    let plan = plan_connect(&d, &limits(), a, bid).unwrap();
    assert_eq!(plan.document.cable_mismatch(&plan.document.cables[0]), None);
}

#[test]
fn catalogue_entries_keep_genders() {
    let (d, id) = one_port();
    let d = plan_set_port_gender(&d, id, Gender::Female)
        .unwrap()
        .document;
    let device = d.racks[0].devices[0].id;
    let d = plan_save_model(
        &d,
        device,
        ModelText::parse("").unwrap(),
        ModelText::parse("Box").unwrap(),
    )
    .unwrap()
    .document;
    assert_eq!(d.catalog[0].ports[0].gender, Gender::Female);
    let model = d.catalog[0].id;
    let rack = d.racks[0].id;
    let d = plan_device_drop(&d, &limits(), &DeviceSource::Model(model), rack, 10)
        .unwrap()
        .document;
    assert!(
        d.racks[0]
            .devices
            .iter()
            .all(|x| x.ports[0].gender == Gender::Female)
    );
}

#[test]
fn genders_survive_files_and_default_when_missing() {
    let (d, id) = one_port();
    let d = plan_set_port_gender(&d, id, Gender::Other)
        .unwrap()
        .document;
    let json = to_json(&d);
    assert!(json.starts_with("{\n  \"format_version\": 5,"), "{json}");
    assert!(json.contains("\"gender\": \"other\""), "{json}");
    assert_eq!(from_json(&json, &limits()), Ok(d.clone()));

    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["format_version"] = 4.into();
    strip_gender(&mut value);
    let v4 = value.to_string();
    assert!(!v4.contains("gender"), "{v4}");
    let loaded = from_json(&v4, &limits()).unwrap();
    assert_eq!(loaded.port(id).unwrap().2.gender, Gender::Unspecified);
}

/// Removes every `gender` field, as a version-4 file would have none.
fn strip_gender(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("gender");
            map.values_mut().for_each(strip_gender);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_gender),
        _ => {}
    }
}
