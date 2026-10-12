//! The example station a first-time user sees: a small test station that shows every feature
//! (racks of different heights, catalogue models placed more than once, ad-hoc equipment, all
//! connector types and genders, coloured cables between racks, and one connector mismatch).
//! It is built through the ordinary edit plans, so it obeys every rule. Its models come in a
//! catalogue of their own, which the app merges into the user's.

use crate::edit::{
    DeviceSource, Plan, plan_connect, plan_device_drop, plan_rename_cable, plan_rename_device,
    plan_save_model, plan_set_cable_color,
};
use crate::ids::{DeviceId, PortId, RackId};
use crate::limits::Limits;
use crate::model::{Catalog, Device, DeviceKind, Document, Gender, Port, PortKind, Rack, Rgb};
use crate::name::{DocumentName, ModelText, Name};

use Gender::{Female, Male};
use PortKind::*;

/// (name, row, col, type, gender) of one port.
type PortSpec = (&'static str, u32, u32, PortKind, Gender);

const RED: u32 = 0xd93636;
const BLACK: u32 = 0x2b2b2b;
const GREEN: u32 = 0x2bb673;
const ORANGE: u32 = 0xe07b20;
const YELLOW: u32 = 0xe0c020;
const BLUE: u32 = 0x2f6fd6;

/// The example station and the catalogue its devices are linked to.
pub fn example_station(limits: &Limits) -> (Document, Catalog) {
    let name = |s: &str| Name::parse(s, limits).expect("example names are valid");
    let device = |n: &str, bottom_u, height_u, color: u32, ports: &[PortSpec]| Device {
        id: DeviceId::new(),
        name: name(n),
        bottom_u,
        height_u,
        color: rgb(color),
        kind: DeviceKind::AdHoc,
        ports: ports
            .iter()
            .map(|&(n, row, col, kind, gender)| Port {
                id: PortId::new(),
                name: name(n),
                row,
                col,
                kind,
                gender,
            })
            .collect(),
    };
    let rack = |n: &str, height_u, devices| Rack {
        id: RackId::new(),
        name: name(n),
        height_u,
        devices,
    };

    let mut doc = Document {
        name: DocumentName::parse("Example Station").expect("valid document name"),
        racks: vec![
            rack(
                "Measure",
                24,
                vec![
                    device(
                        "Switch",
                        22,
                        1,
                        0x2f3640,
                        &[
                            ("P1", 0, 0, Lan, Female),
                            ("P2", 0, 1, Lan, Female),
                            ("P3", 0, 2, Lan, Female),
                            ("P4", 0, 3, Lan, Female),
                            ("P5", 0, 4, Lan, Female),
                        ],
                    ),
                    device(
                        "SigGen",
                        18,
                        2,
                        0xb9bec4,
                        &[
                            ("CH1", 0, 0, Bnc, Female),
                            ("CH2", 0, 1, Bnc, Female),
                            ("SYNC", 0, 2, Bnc, Female),
                            ("USB", 1, 3, Usb, Female),
                            ("LAN", 1, 4, Lan, Female),
                        ],
                    ),
                    device(
                        "Scope",
                        12,
                        4,
                        0x4a6278,
                        &[
                            ("CH1", 0, 0, Bnc, Female),
                            ("CH2", 0, 1, Bnc, Female),
                            ("CH3", 0, 2, Bnc, Female),
                            ("CH4", 0, 3, Bnc, Female),
                            ("TRIG", 0, 4, Bnc, Female),
                            ("USB", 3, 3, Usb, Female),
                            ("LAN", 3, 4, Lan, Female),
                        ],
                    ),
                    device(
                        "DMM",
                        8,
                        2,
                        0xc8ccd0,
                        &[
                            ("GPIB", 0, 0, Gpib, Female),
                            ("LAN", 1, 0, Lan, Female),
                            ("LO", 0, 4, Banana, Female),
                            ("HI", 1, 4, Banana, Female),
                        ],
                    ),
                ],
            ),
            rack(
                "Power",
                18,
                vec![
                    device(
                        "PSU1",
                        14,
                        2,
                        0xd9dcdf,
                        &[
                            ("OUT+", 0, 0, Banana, Female),
                            ("OUT-", 0, 1, Banana, Female),
                            ("LAN", 1, 4, Lan, Female),
                        ],
                    ),
                    device(
                        "PDU",
                        2,
                        1,
                        0x1e1e1e,
                        &[
                            ("IN", 0, 0, Power, Male),
                            ("L1", 0, 1, Power, Female),
                            ("L2", 0, 2, Power, Female),
                            ("L3", 0, 3, Power, Female),
                            ("L4", 0, 4, Power, Female),
                        ],
                    ),
                ],
            ),
            rack(
                "DUT",
                12,
                vec![device(
                    "Fixture",
                    5,
                    3,
                    0xe8a33d,
                    &[
                        ("PWR+", 0, 0, Banana, Female),
                        ("PWR-", 0, 1, Banana, Female),
                        ("SENSE", 0, 2, Banana, Female),
                        ("PROBE", 0, 4, Bnc, Male),
                        ("RF_IN", 1, 0, Sma, Female),
                        ("RF_OUT", 1, 1, Sma, Female),
                        ("ANT", 1, 2, NType, Female),
                        ("CTRL", 2, 0, DSub, Female),
                        ("AUX", 2, 2, Other, Gender::Other),
                        ("SPARE", 2, 3, Unspecified, Gender::Unspecified),
                        ("ETH", 2, 4, Lan, Female),
                    ],
                )],
            ),
        ],
        cables: Vec::new(),
    };
    let mut catalog = Catalog::default();

    // The instruments become catalogue models (the PDU and the fixture stay ad hoc).
    for (device_name, manufacturer, model) in [
        ("Switch", "", "5-port LAN switch"),
        ("SigGen", "Keysight", "33522B"),
        ("Scope", "Rohde & Schwarz", "RTB2004"),
        ("DMM", "Keysight", "34465A"),
        ("PSU1", "Keysight", "E36313A"),
    ] {
        let device = device_id(&doc, device_name);
        let text = |s| ModelText::parse(s).expect("valid model text");
        let plan = plan_save_model(&doc, &catalog, device, text(manufacturer), text(model))
            .expect("the example follows every rule");
        (doc, catalog) = (plan.document, plan.catalog);
    }

    // A second power supply, placed from the catalogue.
    let psu_model = match find_device(&doc, "PSU1").kind {
        DeviceKind::Model(m) => m,
        DeviceKind::AdHoc => unreachable!("PSU1 was saved as a model"),
    };
    let power = doc.racks[1].id;
    let entry = catalog
        .model(psu_model)
        .expect("PSU1's model was saved")
        .clone();
    let plan = plan_device_drop(&doc, limits, &DeviceSource::Model(entry), power, 10);
    let placed = match apply_subject(&mut doc, plan) {
        Some(crate::edit::ObjectId::Device(d)) => d,
        _ => unreachable!("a device drop creates a device"),
    };
    doc = apply(plan_rename_device(&doc, placed, name("PSU2")));

    for (cable, from, to, color) in [
        // BNC to SMA: allowed, but flagged (an adapter is needed).
        ("RF_IN", ("SigGen", "CH1"), ("Fixture", "RF_IN"), ORANGE),
        ("PROBE", ("Fixture", "PROBE"), ("Scope", "CH1"), BLUE),
        ("TRIG", ("SigGen", "SYNC"), ("Scope", "TRIG"), BLUE),
        ("PWR+", ("PSU1", "OUT+"), ("Fixture", "PWR+"), RED),
        ("PWR-", ("PSU1", "OUT-"), ("Fixture", "PWR-"), BLACK),
        ("SENSE", ("DMM", "HI"), ("Fixture", "SENSE"), YELLOW),
        ("LAN1", ("SigGen", "LAN"), ("Switch", "P1"), GREEN),
        ("LAN2", ("Scope", "LAN"), ("Switch", "P2"), GREEN),
        ("LAN3", ("DMM", "LAN"), ("Switch", "P3"), GREEN),
        ("LAN4", ("PSU1", "LAN"), ("Switch", "P4"), GREEN),
        ("LAN5", ("Fixture", "ETH"), ("Switch", "P5"), GREEN),
    ] {
        let (a, b) = (port_id(&doc, from), port_id(&doc, to));
        let plan = plan_connect(&doc, limits, a, b);
        let id = match apply_subject(&mut doc, plan) {
            Some(crate::edit::ObjectId::Cable(c)) => c,
            _ => unreachable!("connecting creates a cable"),
        };
        doc = apply(plan_rename_cable(&doc, id, name(cable)));
        doc = apply(plan_set_cable_color(&doc, id, rgb(color)));
    }
    (doc, catalog)
}

fn rgb(hex: u32) -> Rgb {
    Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

fn apply(plan: Result<Plan, crate::edit::Rejection>) -> Document {
    plan.expect("the example follows every rule").document
}

/// Applies `plan` to `doc` and returns what it created.
fn apply_subject(
    doc: &mut Document,
    plan: Result<Plan, crate::edit::Rejection>,
) -> Option<crate::edit::ObjectId> {
    let plan = plan.expect("the example follows every rule");
    *doc = plan.document;
    plan.subject
}

fn find_device<'a>(doc: &'a Document, name: &str) -> &'a Device {
    doc.racks
        .iter()
        .flat_map(|r| &r.devices)
        .find(|d| d.name.as_str() == name)
        .expect("the example names its devices uniquely")
}

fn device_id(doc: &Document, name: &str) -> DeviceId {
    find_device(doc, name).id
}

fn port_id(doc: &Document, (device, port): (&str, &str)) -> PortId {
    find_device(doc, device)
        .ports
        .iter()
        .find(|p| p.name.as_str() == port)
        .expect("the example's cables name existing ports")
        .id
}
