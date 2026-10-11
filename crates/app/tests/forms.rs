use rackwright_core::ids::{DeviceId, PortId, RackId};
use rackwright_core::limits::Limits;
use rackwright_core::model::{Device, DeviceKind, Document, Port, PortKind, Rack, Rgb};
use rackwright_core::name::{DocumentName, Name};

fn limits() -> Limits {
    Limits::default()
}

fn name(s: &str) -> Name {
    Name::parse(s, &limits()).unwrap()
}

fn port(n: &str, row: u32, col: u32) -> Port {
    Port {
        id: PortId::new(),
        name: name(n),
        row,
        col,
        kind: PortKind::default(),
        gender: Default::default(),
    }
}

fn device(n: &str, bottom_u: u32, height_u: u32) -> Device {
    Device {
        id: DeviceId::new(),
        name: name(n),
        bottom_u,
        height_u,
        color: Rgb::NEUTRAL_GREY,
        kind: DeviceKind::default(),
        ports: Vec::new(),
    }
}

fn with_ports(mut d: Device, ports: Vec<Port>) -> Device {
    d.ports = ports;
    d
}

fn rack(n: &str, height_u: u32, devices: Vec<Device>) -> Rack {
    Rack {
        id: RackId::new(),
        name: name(n),
        height_u,
        devices,
    }
}

fn doc(racks: Vec<Rack>) -> Document {
    Document {
        name: DocumentName::parse("Test").unwrap(),
        racks,
        cables: Vec::new(),
        catalog: Vec::new(),
    }
}
use rackwright_app::forms::*;

#[test]
fn names_are_normalised_or_explained() {
    assert_eq!(parse_name(" DMM 1 ", &limits()).unwrap().as_str(), "DMM1");
    assert_eq!(
        parse_name("  ", &limits()),
        Err("Name must not be empty".into())
    );
    assert_eq!(
        parse_name("ABCDEFGHIJK", &limits()),
        Err("Name must be at most 10 characters".into())
    );
}

#[test]
fn heights_are_whole_numbers_of_at_least_one() {
    assert_eq!(parse_height(" 4 "), Ok(4));
    for bad in ["0", "-1", "1.5", "", "two"] {
        assert!(parse_height(bad).is_err(), "{bad}");
    }
}

#[test]
fn new_device_input_needs_name_and_height() {
    let (n, h) = new_device_input("DMM", "2", &limits()).unwrap();
    assert_eq!((n.as_str(), h), ("DMM", 2));
    assert!(new_device_input("", "2", &limits()).is_err());
    assert!(new_device_input("DMM", "0", &limits()).is_err());
}

#[test]
fn new_port_names_must_be_free_on_the_device() {
    let d = with_ports(device("D", 1, 1), vec![port("CH1", 0, 0)]);
    let id = d.id;
    let document = doc(vec![rack("R", 42, vec![d])]);
    assert_eq!(
        new_port_input(&document, id, "ch1", &limits()),
        Err("The name 'ch1' is already used on this device".into())
    );
    assert_eq!(
        new_port_input(&document, id, "CH2", &limits())
            .unwrap()
            .as_str(),
        "CH2"
    );
}

#[test]
fn document_names_and_export_file_names() {
    assert!(parse_document_name("Lab 3").is_ok());
    assert_eq!(
        parse_document_name("a/b"),
        Err("Document name must not contain '/'".into())
    );
    let mut d = doc(vec![]);
    d.name = parse_document_name("Lab 3").unwrap();
    assert_eq!(export_file_name(&d), "Lab 3.json");
}

#[test]
fn colours_round_trip_through_the_colour_input() {
    let c = rackwright_core::model::Rgb { r: 1, g: 2, b: 255 };
    assert_eq!(color_to_input(c), "#0102ff");
    assert_eq!(color_from_input("#0102ff"), Some(c));
    assert_eq!(color_from_input("blue"), None);
}
