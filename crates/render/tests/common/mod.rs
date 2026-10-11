#![allow(dead_code)]

use rackwright_core::ids::{CableId, DeviceId, PortId, RackId};
use rackwright_core::limits::Limits;
use rackwright_core::model::{Cable, Device, DeviceKind, Document, Port, PortKind, Rack, Rgb};
use rackwright_core::name::{DocumentName, Name};

pub fn limits() -> Limits {
    Limits::default()
}

pub fn name(s: &str) -> Name {
    Name::parse(s, &limits()).unwrap()
}

pub fn port(n: &str, row: u32, col: u32) -> Port {
    Port {
        id: PortId::new(),
        name: name(n),
        row,
        col,
        kind: PortKind::default(),
        gender: Default::default(),
    }
}

pub fn device(n: &str, bottom_u: u32, height_u: u32) -> Device {
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

pub fn with_ports(mut d: Device, ports: Vec<Port>) -> Device {
    d.ports = ports;
    d
}

pub fn rack(n: &str, height_u: u32, devices: Vec<Device>) -> Rack {
    Rack {
        id: RackId::new(),
        name: name(n),
        height_u,
        devices,
    }
}

pub fn doc(racks: Vec<Rack>) -> Document {
    Document {
        name: DocumentName::parse("Test").unwrap(),
        racks,
        cables: Vec::new(),
        catalog: Vec::new(),
    }
}

pub fn assert_close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "{a} != {b}");
}

pub fn cable(n: &str, a: PortId, b: PortId) -> Cable {
    Cable {
        id: CableId::new(),
        name: name(n),
        color: Rgb::CABLE_BLUE,
        a,
        b,
    }
}
