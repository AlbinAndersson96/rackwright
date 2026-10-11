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

pub fn rack_named<'a>(doc: &'a Document, n: &str) -> &'a Rack {
    doc.racks
        .iter()
        .find(|r| r.name.as_str() == n)
        .unwrap_or_else(|| panic!("no rack {n}"))
}

pub fn device_named<'a>(rack: &'a Rack, n: &str) -> &'a Device {
    rack.devices
        .iter()
        .find(|d| d.name.as_str() == n)
        .unwrap_or_else(|| panic!("no device {n}"))
}

/// `(name, bottom_u)` of every device in the rack, lowest first.
pub fn layout(rack: &Rack) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = rack
        .devices
        .iter()
        .map(|d| (d.name.to_string(), d.bottom_u))
        .collect();
    v.sort_by_key(|(_, bottom)| *bottom);
    v
}

pub fn rack_names(doc: &Document) -> Vec<String> {
    doc.racks.iter().map(|r| r.name.to_string()).collect()
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
