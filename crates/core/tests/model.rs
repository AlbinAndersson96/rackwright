mod common;

use common::*;
use rackwright_core::ids::{DeviceId, PortId};
use rackwright_core::model::{Document, Rgb};

#[test]
fn new_default_is_station1_with_one_42u_rack() {
    let d = Document::new_default(&limits());
    assert_eq!(d.name.as_str(), "Station1");
    assert_eq!(rack_names(&d), vec!["Rack1"]);
    assert_eq!(d.racks[0].height_u, 42);
    assert!(d.racks[0].devices.is_empty());
}

#[test]
fn lookups_find_objects_by_id() {
    let p = port("CH1", 0, 0);
    let pid = p.id;
    let dev = with_ports(device("DMM", 3, 2), vec![p]);
    let did = dev.id;
    let r = rack("Rack1", 42, vec![dev]);
    let rid = r.id;
    let d = doc(vec![r]);

    assert_eq!(d.rack(rid).unwrap().name.as_str(), "Rack1");
    let (r, dv) = d.device(did).unwrap();
    assert_eq!((r.id, dv.name.as_str()), (rid, "DMM"));
    let (r, dv, p) = d.port(pid).unwrap();
    assert_eq!((r.id, dv.id, p.name.as_str()), (rid, did, "CH1"));

    assert!(d.device(DeviceId::new()).is_none());
    assert!(d.port(PortId::new()).is_none());
}

#[test]
fn top_u_and_port_rows() {
    let dev = device("PSU", 10, 3);
    assert_eq!(dev.top_u(), 12);
    assert_eq!(dev.port_rows(&limits()), 3);
}

#[test]
fn devices_are_listed_top_down() {
    let r = rack(
        "R",
        42,
        vec![
            device("Low", 1, 1),
            device("High", 30, 2),
            device("Mid", 10, 1),
        ],
    );
    let names: Vec<&str> = r
        .devices_top_down()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, vec!["High", "Mid", "Low"]);
}

#[test]
fn ports_are_listed_in_reading_order() {
    // Row 0 is the bottom row, so the top row (row 1) is read first.
    let dev = with_ports(
        device("D", 1, 2),
        vec![
            port("B0", 0, 3),
            port("T4", 1, 4),
            port("A0", 0, 0),
            port("T1", 1, 1),
        ],
    );
    let names: Vec<&str> = dev
        .ports_in_reading_order()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["T1", "T4", "A0", "B0"]);
}

#[test]
fn rgb_hex_round_trip() {
    assert_eq!(Rgb::NEUTRAL_GREY.to_hex(), "#a0a0a0");
    let c = Rgb {
        r: 0x12,
        g: 0xab,
        b: 0xff,
    };
    assert_eq!(Rgb::from_hex(&c.to_hex()), Some(c));
    assert_eq!(Rgb::from_hex("#12ABFF"), Some(c));
}

#[test]
fn rgb_rejects_malformed_hex() {
    for bad in [
        "#12345", "123456", "#gg0000", "#+f0000", "#ÅÅ00", "#1234567",
    ] {
        assert_eq!(Rgb::from_hex(bad), None, "{bad}");
    }
}
