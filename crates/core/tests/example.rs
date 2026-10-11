mod common;

use std::collections::HashSet;

use common::*;
use rackwright_core::example::example_document;
use rackwright_core::file_format::{from_json, to_json};
use rackwright_core::model::{DeviceKind, Gender, PortKind};
use rackwright_core::validate::validate;

#[test]
fn the_example_is_valid_and_survives_a_file_round_trip() {
    let d = example_document(&limits());
    assert_eq!(validate(&d, &limits()), Ok(()));
    assert_eq!(from_json(&to_json(&d), &limits()), Ok(d.clone()));
    assert_eq!(d.name.as_str(), "Example Station");
}

#[test]
fn the_example_shows_every_feature() {
    let d = example_document(&limits());
    let devices: Vec<_> = d.racks.iter().flat_map(|r| &r.devices).collect();
    let ports: Vec<_> = devices.iter().flat_map(|x| &x.ports).collect();

    assert!(d.racks.len() >= 3, "a row of racks");
    let heights: HashSet<u32> = d.racks.iter().map(|r| r.height_u).collect();
    assert!(heights.len() >= 2, "racks of different heights");

    // Catalogue: several models, one placed twice, and ad-hoc devices too.
    assert!(d.catalog.len() >= 4);
    let linked: Vec<_> = devices
        .iter()
        .filter_map(|x| match x.kind {
            DeviceKind::Model(m) => Some(m),
            DeviceKind::AdHoc => None,
        })
        .collect();
    let distinct: HashSet<_> = linked.iter().collect();
    assert!(
        linked.len() > distinct.len(),
        "a model placed more than once"
    );
    assert!(devices.iter().any(|x| x.kind == DeviceKind::AdHoc));

    // Every connector type and gender appears.
    let kinds: HashSet<PortKind> = ports.iter().map(|p| p.kind).collect();
    for kind in PortKind::ALL {
        assert!(kinds.contains(&kind), "no {kind:?} port");
    }
    let genders: HashSet<Gender> = ports.iter().map(|p| p.gender).collect();
    for gender in Gender::ALL {
        assert!(genders.contains(&gender), "no {gender:?} port");
    }

    // Cables: several, in several colours, across racks, one mismatched.
    assert!(d.cables.len() >= 8);
    let colours: HashSet<_> = d.cables.iter().map(|c| c.color.to_hex()).collect();
    assert!(colours.len() >= 3);
    let rack_of = |p| d.port(p).map(|(r, _, _)| r.id);
    assert!(d.cables.iter().any(|c| rack_of(c.a) != rack_of(c.b)));
    let mismatched = d
        .cables
        .iter()
        .filter(|c| d.cable_mismatch(c).is_some())
        .count();
    assert_eq!(mismatched, 1, "exactly one ⚠ to discover");

    // Device colours vary.
    let device_colours: HashSet<_> = devices.iter().map(|x| x.color.to_hex()).collect();
    assert!(device_colours.len() >= 4);
}

#[test]
fn each_example_has_fresh_ids() {
    let (a, b) = (example_document(&limits()), example_document(&limits()));
    assert_ne!(a.racks[0].id, b.racks[0].id);
    assert_eq!(to_json(&a).len(), to_json(&b).len(), "same content");
}
