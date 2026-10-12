use std::cmp::Reverse;

use crate::ids::{CableId, DeviceId, ModelId, PortId, RackId};
use crate::limits::Limits;
use crate::name::{DocumentName, ModelText, Name};

pub const DEFAULT_RACK_HEIGHT_U: u32 = 42;
pub const DEFAULT_DOCUMENT_NAME: &str = "Station1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const NEUTRAL_GREY: Rgb = Rgb {
        r: 0xa0,
        g: 0xa0,
        b: 0xa0,
    };
    /// The colour of a new cable.
    pub const CABLE_BLUE: Rgb = Rgb {
        r: 0x2f,
        g: 0x6f,
        b: 0xd6,
    };

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Parses exactly `#rrggbb` (hex digits, either case).
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let hex = s.strip_prefix('#')?;
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Rgb {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
        })
    }
}

/// Where a device came from: entered by hand, or placed from a catalogue entry (whose
/// contents were copied into it; the link is informational).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceKind {
    #[default]
    AdHoc,
    Model(ModelId),
}

/// A connector's gender (independent of its type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Gender {
    #[default]
    Unspecified,
    Male,
    Female,
    /// Genderless or hermaphroditic connectors.
    Other,
}

impl Gender {
    pub const ALL: [Gender; 4] = [
        Gender::Unspecified,
        Gender::Male,
        Gender::Female,
        Gender::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Gender::Unspecified => "Unspecified",
            Gender::Male => "Male",
            Gender::Female => "Female",
            Gender::Other => "Other (genderless)",
        }
    }

    /// A stable identifier (the file format's spelling, also used for UI option values).
    pub fn key(self) -> &'static str {
        match self {
            Gender::Unspecified => "unspecified",
            Gender::Male => "male",
            Gender::Female => "female",
            Gender::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Option<Gender> {
        Gender::ALL.into_iter().find(|g| g.key() == key)
    }
}

/// A port of a catalogue entry: copied, with a new ID, into each placed device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPort {
    pub name: Name,
    pub row: u32,
    pub col: u32,
    pub kind: PortKind,
    pub gender: Gender,
}

/// A model in the equipment catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: ModelId,
    /// May be empty.
    pub manufacturer: ModelText,
    /// Never empty.
    pub model: ModelText,
    pub height_u: u32,
    pub color: Rgb,
    pub ports: Vec<ModelPort>,
}

impl CatalogEntry {
    /// "Manufacturer Model", or the model alone without a manufacturer.
    pub fn display_name(&self) -> String {
        display_name(&self.manufacturer, &self.model)
    }

    /// The identity compared for uniqueness (case-insensitive).
    pub fn key(&self) -> String {
        self.display_name().to_lowercase()
    }
}

/// The equipment catalogue. It is kept apart from station documents; devices link to its
/// entries by ID, and a link may name an entry that is not here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    pub fn model(&self, id: ModelId) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The entry with the same manufacturer + model as `key` (see `CatalogEntry::key`).
    pub fn by_key(&self, key: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.key() == key)
    }
}

pub fn display_name(manufacturer: &ModelText, model: &ModelText) -> String {
    if manufacturer.is_empty() {
        model.to_string()
    } else {
        format!("{manufacturer} {model}")
    }
}

/// A port's connector type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PortKind {
    #[default]
    Unspecified,
    Bnc,
    Sma,
    NType,
    Banana,
    Usb,
    Lan,
    Gpib,
    DSub,
    Power,
    Other,
}

impl PortKind {
    /// Every type, in the order the UI lists them.
    pub const ALL: [PortKind; 11] = [
        PortKind::Unspecified,
        PortKind::Bnc,
        PortKind::Sma,
        PortKind::NType,
        PortKind::Banana,
        PortKind::Usb,
        PortKind::Lan,
        PortKind::Gpib,
        PortKind::DSub,
        PortKind::Power,
        PortKind::Other,
    ];

    /// The name shown to users.
    pub fn label(self) -> &'static str {
        match self {
            PortKind::Unspecified => "Unspecified",
            PortKind::Bnc => "BNC",
            PortKind::Sma => "SMA",
            PortKind::NType => "N-type",
            PortKind::Banana => "Banana",
            PortKind::Usb => "USB",
            PortKind::Lan => "LAN (RJ45)",
            PortKind::Gpib => "GPIB",
            PortKind::DSub => "D-sub",
            PortKind::Power => "Power (IEC)",
            PortKind::Other => "Other",
        }
    }

    /// A stable identifier (the file format's spelling, also used for UI option values).
    pub fn key(self) -> &'static str {
        match self {
            PortKind::Unspecified => "unspecified",
            PortKind::Bnc => "bnc",
            PortKind::Sma => "sma",
            PortKind::NType => "n_type",
            PortKind::Banana => "banana",
            PortKind::Usb => "usb",
            PortKind::Lan => "lan",
            PortKind::Gpib => "gpib",
            PortKind::DSub => "d_sub",
            PortKind::Power => "power",
            PortKind::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Option<PortKind> {
        PortKind::ALL.into_iter().find(|k| k.key() == key)
    }

    /// A real connector type: only these can mismatch.
    pub fn is_specific(self) -> bool {
        !matches!(self, PortKind::Unspecified | PortKind::Other)
    }
}

/// A port in cell (`row`, `col`) of its device's front face. Row 0 is the bottom row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub id: PortId,
    pub name: Name,
    pub row: u32,
    pub col: u32,
    pub kind: PortKind,
    pub gender: Gender,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: Name,
    pub bottom_u: u32,
    pub height_u: u32,
    pub color: Rgb,
    pub kind: DeviceKind,
    pub ports: Vec<Port>,
}

impl Device {
    pub fn top_u(&self) -> u32 {
        self.bottom_u + self.height_u - 1
    }

    pub fn port_rows(&self, limits: &Limits) -> u32 {
        self.height_u * limits.port_rows_per_u
    }

    /// Top row first, then left to right.
    pub fn ports_in_reading_order(&self) -> Vec<&Port> {
        let mut ports: Vec<&Port> = self.ports.iter().collect();
        ports.sort_by_key(|p| (Reverse(p.row), p.col));
        ports
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rack {
    pub id: RackId,
    pub name: Name,
    pub height_u: u32,
    pub devices: Vec<Device>,
}

impl Rack {
    /// Physical order: highest U first.
    pub fn devices_top_down(&self) -> Vec<&Device> {
        let mut devices: Vec<&Device> = self.devices.iter().collect();
        devices.sort_by_key(|d| Reverse(d.bottom_u));
        devices
    }
}

/// A cable between two different ports. The order of `a` and `b` carries no meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cable {
    pub id: CableId,
    pub name: Name,
    pub color: Rgb,
    pub a: PortId,
    pub b: PortId,
}

impl Cable {
    pub fn ends(&self) -> [PortId; 2] {
        [self.a, self.b]
    }

    pub fn touches(&self, port: PortId) -> bool {
        self.a == port || self.b == port
    }
}

/// `racks` order is the left-to-right order of the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub name: DocumentName,
    pub racks: Vec<Rack>,
    pub cables: Vec<Cable>,
}

impl Document {
    pub fn new_default(limits: &Limits) -> Document {
        Document {
            name: DocumentName::parse(DEFAULT_DOCUMENT_NAME).expect("default name is valid"),
            racks: vec![Rack {
                id: RackId::new(),
                name: Name::parse("Rack1", limits).expect("default rack name is valid"),
                height_u: DEFAULT_RACK_HEIGHT_U,
                devices: Vec::new(),
            }],
            cables: Vec::new(),
        }
    }

    pub fn rack(&self, id: RackId) -> Option<&Rack> {
        self.racks.iter().find(|r| r.id == id)
    }

    pub fn device(&self, id: DeviceId) -> Option<(&Rack, &Device)> {
        let (ri, di) = self.device_location(id)?;
        let rack = &self.racks[ri];
        Some((rack, &rack.devices[di]))
    }

    pub fn port(&self, id: PortId) -> Option<(&Rack, &Device, &Port)> {
        let (ri, di, pi) = self.port_location(id)?;
        let rack = &self.racks[ri];
        let device = &rack.devices[di];
        Some((rack, device, &device.ports[pi]))
    }

    pub fn cable(&self, id: CableId) -> Option<&Cable> {
        self.cables.iter().find(|c| c.id == id)
    }

    /// The types of `cable`'s ends when both are specific and differ (a warning, not an error).
    pub fn cable_mismatch(&self, cable: &Cable) -> Option<(PortKind, PortKind)> {
        let (_, _, a) = self.port(cable.a)?;
        let (_, _, b) = self.port(cable.b)?;
        (a.kind.is_specific() && b.kind.is_specific() && a.kind != b.kind)
            .then_some((a.kind, b.kind))
    }

    /// The cable plugged into `port`, if any.
    pub fn cable_at_port(&self, port: PortId) -> Option<&Cable> {
        self.cables.iter().find(|c| c.touches(port))
    }

    pub(crate) fn rack_index(&self, id: RackId) -> Option<usize> {
        self.racks.iter().position(|r| r.id == id)
    }

    pub(crate) fn device_location(&self, id: DeviceId) -> Option<(usize, usize)> {
        self.racks.iter().enumerate().find_map(|(ri, rack)| {
            rack.devices
                .iter()
                .position(|d| d.id == id)
                .map(|di| (ri, di))
        })
    }

    pub(crate) fn port_location(&self, id: PortId) -> Option<(usize, usize, usize)> {
        self.racks.iter().enumerate().find_map(|(ri, rack)| {
            rack.devices.iter().enumerate().find_map(|(di, device)| {
                device
                    .ports
                    .iter()
                    .position(|p| p.id == id)
                    .map(|pi| (ri, di, pi))
            })
        })
    }
}
