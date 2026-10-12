use crate::edit::cable::drop_dangling_cables;
use crate::edit::catalog::device_from_model;
use crate::edit::{ObjectId, Plan, Rejection};
use crate::ids::{DeviceId, RackId};
use crate::limits::Limits;
use crate::model::{CatalogEntry, Device, DeviceKind, Document, Rgb};
use crate::name::{Name, auto_rename};
use crate::placement::{self, Occupant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSource {
    New {
        name: Name,
        height_u: u32,
    },
    Existing(DeviceId),
    /// A new device with the contents of a catalogue entry, linked to it.
    Model(CatalogEntry),
}

/// Places a new or existing device with its bottom at `bottom_u` (clamped into the rack),
/// pushing other devices away from its centre and auto-renaming it on a name clash.
pub fn plan_device_drop(
    doc: &Document,
    limits: &Limits,
    source: &DeviceSource,
    target_rack: RackId,
    bottom_u: u32,
) -> Result<Plan, Rejection> {
    let mut document = doc.clone();
    let mut device = match source {
        DeviceSource::New { name, height_u } => {
            if *height_u == 0 {
                return Err(Rejection::ZeroHeight);
            }
            Device {
                id: DeviceId::new(),
                name: name.clone(),
                bottom_u: 1,
                height_u: *height_u,
                color: Rgb::NEUTRAL_GREY,
                kind: DeviceKind::default(),
                ports: Vec::new(),
            }
        }
        DeviceSource::Existing(id) => {
            let (ri, di) = document.device_location(*id).ok_or(Rejection::NotFound)?;
            document.racks[ri].devices.remove(di)
        }
        DeviceSource::Model(entry) => device_from_model(entry, limits),
    };

    let rack_index = document
        .rack_index(target_rack)
        .ok_or(Rejection::NotFound)?;
    let rack = &mut document.racks[rack_index];
    let bottom = placement::clamp_bottom(bottom_u, device.height_u, rack.height_u)?;
    let others = occupants(&rack.devices);
    let new_bottoms = placement::push_for_drop(&others, bottom, device.height_u, rack.height_u)?;
    apply_bottoms(&mut rack.devices, &new_bottoms);

    device.name = auto_rename(
        &device.name,
        |c| rack.devices.iter().any(|d| d.name.same_as(c)),
        limits,
    );
    device.bottom_u = bottom;
    let subject = Some(ObjectId::Device(device.id));
    rack.devices.push(device);
    Ok(Plan { document, subject })
}

pub fn plan_remove_device(doc: &Document, device_id: DeviceId) -> Result<Plan, Rejection> {
    let (ri, di) = doc.device_location(device_id).ok_or(Rejection::NotFound)?;
    let mut document = doc.clone();
    document.racks[ri].devices.remove(di);
    drop_dangling_cables(&mut document);
    Ok(Plan {
        document,
        subject: None,
    })
}

pub fn plan_rename_device(
    doc: &Document,
    device_id: DeviceId,
    name: Name,
) -> Result<Plan, Rejection> {
    let (ri, di) = doc.device_location(device_id).ok_or(Rejection::NotFound)?;
    if doc.racks[ri]
        .devices
        .iter()
        .any(|d| d.id != device_id && d.name.same_as(&name))
    {
        return Err(Rejection::NameTaken(name.to_string()));
    }
    let mut document = doc.clone();
    document.racks[ri].devices[di].name = name;
    Ok(Plan {
        document,
        subject: Some(ObjectId::Device(device_id)),
    })
}

pub fn plan_set_device_color(
    doc: &Document,
    device_id: DeviceId,
    color: Rgb,
) -> Result<Plan, Rejection> {
    let (ri, di) = doc.device_location(device_id).ok_or(Rejection::NotFound)?;
    let mut document = doc.clone();
    document.racks[ri].devices[di].color = color;
    Ok(Plan {
        document,
        subject: Some(ObjectId::Device(device_id)),
    })
}

/// Growing keeps the bottom fixed and pushes devices above upward.
/// Shrinking is rejected if any port would fall off the smaller face.
pub fn plan_set_device_height(
    doc: &Document,
    limits: &Limits,
    device_id: DeviceId,
    height_u: u32,
) -> Result<Plan, Rejection> {
    let (ri, di) = doc.device_location(device_id).ok_or(Rejection::NotFound)?;
    if height_u == 0 {
        return Err(Rejection::ZeroHeight);
    }
    let mut document = doc.clone();
    let rack = &mut document.racks[ri];
    let (bottom_u, current_height) = (rack.devices[di].bottom_u, rack.devices[di].height_u);

    if height_u < current_height {
        let rows = height_u * limits.port_rows_per_u;
        if rack.devices[di].ports.iter().any(|p| p.row >= rows) {
            return Err(Rejection::PortsOutside);
        }
    } else if height_u > current_height {
        if u64::from(bottom_u) + u64::from(height_u) - 1 > u64::from(rack.height_u) {
            return Err(Rejection::NoRoomAbove);
        }
        let others = occupants(rack.devices.iter().filter(|d| d.id != device_id));
        let new_bottoms = placement::push_for_growth(&others, bottom_u, height_u, rack.height_u)?;
        apply_bottoms(&mut rack.devices, &new_bottoms);
    }

    rack.devices[di].height_u = height_u;
    Ok(Plan {
        document,
        subject: Some(ObjectId::Device(device_id)),
    })
}

fn occupants<'a>(devices: impl IntoIterator<Item = &'a Device>) -> Vec<Occupant> {
    devices
        .into_iter()
        .map(|d| Occupant {
            id: d.id,
            bottom_u: d.bottom_u,
            height_u: d.height_u,
        })
        .collect()
}

fn apply_bottoms(devices: &mut [Device], bottoms: &[(DeviceId, u32)]) {
    for device in devices.iter_mut() {
        if let Some((_, bottom)) = bottoms.iter().find(|(id, _)| *id == device.id) {
            device.bottom_u = *bottom;
        }
    }
}
