//! Full invariant checks for documents and catalogues that did not come from the editor
//! (imports, storage).
//! All arithmetic is in u64 so absurd values cannot overflow.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::ids::PortId;
use crate::limits::Limits;
use crate::model::{Cable, Catalog, CatalogEntry, Device, Document, Rack};
use crate::name::Name;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("The ID {0} is used more than once")]
    DuplicateId(String),
    #[error("The rack name '{0}' is used more than once")]
    DuplicateRackName(String),
    #[error("Rack '{rack}': the height must be at least 1U")]
    RackZeroHeight { rack: String },
    #[error("Rack '{rack}': the height must be at most {max}U")]
    RackTooTall { rack: String, max: u32 },
    #[error("Rack '{rack}': the device name '{name}' is used more than once")]
    DuplicateDeviceName { rack: String, name: String },
    #[error("Rack '{rack}': device '{device}' must be at least 1U high")]
    DeviceZeroHeight { rack: String, device: String },
    #[error("Rack '{rack}': device '{device}' is outside the rack")]
    DeviceOutsideRack { rack: String, device: String },
    #[error("Rack '{rack}': devices '{a}' and '{b}' overlap at U{u}")]
    DevicesOverlap {
        rack: String,
        a: String,
        b: String,
        u: u32,
    },
    #[error("Device '{device}': the port name '{name}' is used more than once")]
    DuplicatePortName { device: String, name: String },
    #[error("Device '{device}': port '{port}' is outside the device face")]
    PortOutsideGrid { device: String, port: String },
    #[error("Device '{device}': ports '{a}' and '{b}' share a cell")]
    PortsOverlap {
        device: String,
        a: String,
        b: String,
    },
    #[error("The cable name '{0}' is used more than once")]
    DuplicateCableName(String),
    #[error("Cable '{cable}': an end is not a port in this document")]
    CableEndMissing { cable: String },
    #[error("Cable '{cable}': both ends are the same port")]
    CableEndsEqual { cable: String },
    #[error("The model '{0}' is in the catalogue more than once")]
    DuplicateModel(String),
    #[error("Model '{model}': {reason}")]
    InvalidModel { model: String, reason: String },
    #[error("Device '{device}': port '{port}' has two cables, '{a}' and '{b}'")]
    PortHasTwoCables {
        device: String,
        port: String,
        a: String,
        b: String,
    },
}

pub fn validate(doc: &Document, limits: &Limits) -> Result<(), ValidationError> {
    check_unique_ids(doc)?;
    if let Some(name) = first_duplicate(doc.racks.iter().map(|r| &r.name)) {
        return Err(ValidationError::DuplicateRackName(name.to_string()));
    }
    for rack in &doc.racks {
        check_rack(rack, limits)?;
    }
    check_cables(doc)
}

/// The catalogue rules. Devices may link to models that are not here, so stations are not
/// checked against it.
pub fn validate_catalog(catalog: &Catalog, limits: &Limits) -> Result<(), ValidationError> {
    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    for entry in &catalog.entries {
        if !ids.insert(entry.id) {
            return Err(ValidationError::DuplicateId(entry.id.0.to_string()));
        }
        if !keys.insert(entry.key()) {
            return Err(ValidationError::DuplicateModel(entry.display_name()));
        }
        check_entry(entry, limits).map_err(|reason| ValidationError::InvalidModel {
            model: entry.display_name(),
            reason,
        })?;
    }
    Ok(())
}

fn check_unique_ids(doc: &Document) -> Result<(), ValidationError> {
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut insert = |id: Uuid| {
        if seen.insert(id) {
            Ok(())
        } else {
            Err(ValidationError::DuplicateId(id.to_string()))
        }
    };
    for rack in &doc.racks {
        insert(rack.id.0)?;
        for device in &rack.devices {
            insert(device.id.0)?;
            for port in &device.ports {
                insert(port.id.0)?;
            }
        }
    }
    for cable in &doc.cables {
        insert(cable.id.0)?;
    }
    Ok(())
}

/// The first name whose case-insensitive form was already seen.
fn first_duplicate<'a>(mut names: impl Iterator<Item = &'a Name>) -> Option<&'a Name> {
    let mut seen = HashSet::new();
    names.find(|n| !seen.insert(n.key()))
}

fn top(device: &Device) -> u64 {
    u64::from(device.bottom_u) + u64::from(device.height_u) - 1
}

fn check_rack(rack: &Rack, limits: &Limits) -> Result<(), ValidationError> {
    let rack_name = rack.name.to_string();
    if rack.height_u == 0 {
        return Err(ValidationError::RackZeroHeight { rack: rack_name });
    }
    if rack.height_u > limits.max_rack_height_u {
        return Err(ValidationError::RackTooTall {
            rack: rack_name,
            max: limits.max_rack_height_u,
        });
    }
    if let Some(name) = first_duplicate(rack.devices.iter().map(|d| &d.name)) {
        return Err(ValidationError::DuplicateDeviceName {
            rack: rack_name,
            name: name.to_string(),
        });
    }
    for device in &rack.devices {
        if device.height_u == 0 {
            return Err(ValidationError::DeviceZeroHeight {
                rack: rack_name,
                device: device.name.to_string(),
            });
        }
        if device.bottom_u < 1 || top(device) > u64::from(rack.height_u) {
            return Err(ValidationError::DeviceOutsideRack {
                rack: rack_name,
                device: device.name.to_string(),
            });
        }
        check_ports(device, limits)?;
    }
    let mut sorted: Vec<&Device> = rack.devices.iter().collect();
    sorted.sort_by_key(|d| d.bottom_u);
    for pair in sorted.windows(2) {
        let (lower, upper) = (pair[0], pair[1]);
        if u64::from(upper.bottom_u) <= top(lower) {
            return Err(ValidationError::DevicesOverlap {
                rack: rack_name,
                a: lower.name.to_string(),
                b: upper.name.to_string(),
                u: upper.bottom_u,
            });
        }
    }
    Ok(())
}

fn check_ports(device: &Device, limits: &Limits) -> Result<(), ValidationError> {
    let device_name = device.name.to_string();
    if let Some(name) = first_duplicate(device.ports.iter().map(|p| &p.name)) {
        return Err(ValidationError::DuplicatePortName {
            device: device_name,
            name: name.to_string(),
        });
    }
    let rows = u64::from(device.height_u) * u64::from(limits.port_rows_per_u);
    for port in &device.ports {
        if u64::from(port.row) >= rows || port.col >= limits.port_cols {
            return Err(ValidationError::PortOutsideGrid {
                device: device_name,
                port: port.name.to_string(),
            });
        }
    }
    let mut cells = HashMap::new();
    for port in &device.ports {
        if let Some(other) = cells.insert((port.row, port.col), &port.name) {
            return Err(ValidationError::PortsOverlap {
                device: device_name,
                a: other.to_string(),
                b: port.name.to_string(),
            });
        }
    }
    Ok(())
}

fn check_cables(doc: &Document) -> Result<(), ValidationError> {
    if let Some(name) = first_duplicate(doc.cables.iter().map(|c| &c.name)) {
        return Err(ValidationError::DuplicateCableName(name.to_string()));
    }
    let mut used: HashMap<PortId, &Cable> = HashMap::new();
    for cable in &doc.cables {
        let cable_name = cable.name.to_string();
        if cable.a == cable.b {
            return Err(ValidationError::CableEndsEqual { cable: cable_name });
        }
        for end in cable.ends() {
            let Some((_, device, port)) = doc.port(end) else {
                return Err(ValidationError::CableEndMissing { cable: cable_name });
            };
            if let Some(other) = used.insert(end, cable) {
                return Err(ValidationError::PortHasTwoCables {
                    device: device.name.to_string(),
                    port: port.name.to_string(),
                    a: other.name.to_string(),
                    b: cable_name,
                });
            }
        }
    }
    Ok(())
}

/// The device rules, applied to an entry's contents.
fn check_entry(entry: &CatalogEntry, limits: &Limits) -> Result<(), String> {
    if entry.model.is_empty() {
        return Err("the model must not be empty".into());
    }
    if entry.height_u == 0 || entry.height_u > limits.max_rack_height_u {
        return Err(format!(
            "the height must be between 1U and {}U",
            limits.max_rack_height_u
        ));
    }
    if let Some(name) = first_duplicate(entry.ports.iter().map(|p| &p.name)) {
        return Err(format!("the port name '{name}' is used more than once"));
    }
    let rows = u64::from(entry.height_u) * u64::from(limits.port_rows_per_u);
    let mut cells = HashMap::new();
    for port in &entry.ports {
        if u64::from(port.row) >= rows || port.col >= limits.port_cols {
            return Err(format!("port '{}' is outside the device face", port.name));
        }
        if let Some(other) = cells.insert((port.row, port.col), &port.name) {
            return Err(format!("ports '{other}' and '{}' share a cell", port.name));
        }
    }
    Ok(())
}
