use std::collections::HashMap;

use crate::edit::{CatalogPlan, ObjectId, Rejection};
use crate::ids::{DeviceId, ModelId};
use crate::limits::Limits;
use crate::model::{Catalog, CatalogEntry, Device, DeviceKind, Document, ModelPort, display_name};
use crate::name::{ModelText, Name};

/// Makes a catalogue entry from the device's height, colour and ports, and links the device
/// to it.
pub fn plan_save_model(
    doc: &Document,
    catalog: &Catalog,
    device_id: DeviceId,
    manufacturer: ModelText,
    model: ModelText,
) -> Result<CatalogPlan, Rejection> {
    let (ri, di) = doc.device_location(device_id).ok_or(Rejection::NotFound)?;
    check_identity(catalog, None, &manufacturer, &model)?;
    let device = &doc.racks[ri].devices[di];
    let entry = CatalogEntry {
        id: ModelId::new(),
        manufacturer,
        model,
        height_u: device.height_u,
        color: device.color,
        ports: model_ports(device),
    };
    let mut document = doc.clone();
    document.racks[ri].devices[di].kind = DeviceKind::Model(entry.id);
    let mut catalog = catalog.clone();
    catalog.entries.push(entry);
    Ok(CatalogPlan {
        document,
        catalog,
        subject: Some(ObjectId::Device(device_id)),
    })
}

/// Replaces the linked entry's height, colour and ports with the device's. Other devices
/// placed from the entry are not changed.
pub fn plan_update_model(
    doc: &Document,
    catalog: &Catalog,
    device_id: DeviceId,
) -> Result<CatalogPlan, Rejection> {
    let (_, device) = doc.device(device_id).ok_or(Rejection::NotFound)?;
    let DeviceKind::Model(model_id) = device.kind else {
        return Err(Rejection::NotLinked);
    };
    let index = model_index(catalog, model_id).map_err(|_| Rejection::ModelMissing)?;
    let mut catalog = catalog.clone();
    let entry = &mut catalog.entries[index];
    entry.height_u = device.height_u;
    entry.color = device.color;
    entry.ports = model_ports(device);
    Ok(CatalogPlan {
        document: doc.clone(),
        catalog,
        subject: Some(ObjectId::Device(device_id)),
    })
}

pub fn plan_rename_model(
    doc: &Document,
    catalog: &Catalog,
    model_id: ModelId,
    manufacturer: ModelText,
    model: ModelText,
) -> Result<CatalogPlan, Rejection> {
    let index = model_index(catalog, model_id)?;
    check_identity(catalog, Some(model_id), &manufacturer, &model)?;
    let mut catalog = catalog.clone();
    let entry = &mut catalog.entries[index];
    entry.manufacturer = manufacturer;
    entry.model = model;
    Ok(CatalogPlan {
        document: doc.clone(),
        catalog,
        subject: None,
    })
}

/// Removes the entry; devices placed from it become ad hoc.
pub fn plan_remove_model(
    doc: &Document,
    catalog: &Catalog,
    model_id: ModelId,
) -> Result<CatalogPlan, Rejection> {
    let index = model_index(catalog, model_id)?;
    let mut catalog = catalog.clone();
    catalog.entries.remove(index);
    let mut document = doc.clone();
    for device in all_devices(&mut document) {
        if device.kind == DeviceKind::Model(model_id) {
            device.kind = DeviceKind::AdHoc;
        }
    }
    Ok(CatalogPlan {
        document,
        catalog,
        subject: None,
    })
}

/// Replaces `old` with `incoming` (Import catalogue). Each linked device keeps its link when
/// `incoming` has the ID, is relinked when `incoming` has an entry with the same manufacturer
/// + model as its old entry, and becomes ad hoc otherwise.
pub fn plan_replace_catalog(doc: &Document, old: &Catalog, incoming: Catalog) -> CatalogPlan {
    let mut document = doc.clone();
    for device in all_devices(&mut document) {
        let DeviceKind::Model(id) = device.kind else {
            continue;
        };
        if incoming.model(id).is_some() {
            continue;
        }
        device.kind = old
            .model(id)
            .and_then(|e| incoming.by_key(&e.key()))
            .map_or(DeviceKind::AdHoc, |e| DeviceKind::Model(e.id));
    }
    CatalogPlan {
        document,
        catalog: incoming,
        subject: None,
    }
}

/// Merges `incoming` into `catalog` and relinks `incoming_doc` (a station that came with
/// `incoming`). An incoming entry matching an existing one by ID, or else by manufacturer +
/// model, overwrites its contents and text but keeps its ID; any other entry is added.
pub fn merge_catalog(
    catalog: &Catalog,
    incoming: &Catalog,
    incoming_doc: &Document,
) -> (Catalog, Document) {
    let mut merged = catalog.clone();
    let mut relink: HashMap<ModelId, ModelId> = HashMap::new();
    for entry in &incoming.entries {
        let key = entry.key();
        let target = merged
            .entries
            .iter()
            .position(|e| e.id == entry.id)
            .or_else(|| merged.entries.iter().position(|e| e.key() == key));
        match target {
            Some(i) => {
                let kept = merged.entries[i].id;
                merged.entries[i] = CatalogEntry {
                    id: kept,
                    ..entry.clone()
                };
                // An ID match may now share its text with another entry: the incoming one wins.
                merged.entries.retain(|e| e.id == kept || e.key() != key);
                relink.insert(entry.id, kept);
            }
            None => merged.entries.push(entry.clone()),
        }
    }
    let mut document = incoming_doc.clone();
    for device in all_devices(&mut document) {
        if let DeviceKind::Model(id) = device.kind
            && let Some(kept) = relink.get(&id)
        {
            device.kind = DeviceKind::Model(*kept);
        }
    }
    (merged, document)
}

fn all_devices(doc: &mut Document) -> impl Iterator<Item = &mut Device> {
    doc.racks.iter_mut().flat_map(|r| &mut r.devices)
}

/// A new device with the entry's contents (ports get new IDs), linked to it, at U1 (the drop
/// places it). Its name is the model text without whitespace, cut to fit.
pub(crate) fn device_from_model(entry: &CatalogEntry, limits: &Limits) -> Device {
    let compact: String = entry
        .model
        .as_str()
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(limits.name_max_len)
        .collect();
    let name = Name::parse(&compact, limits)
        .or_else(|_| Name::parse("Device", limits))
        .expect("'Device' is a valid name");
    Device {
        id: DeviceId::new(),
        name,
        bottom_u: 1,
        height_u: entry.height_u,
        color: entry.color,
        kind: DeviceKind::Model(entry.id),
        ports: entry
            .ports
            .iter()
            .map(|p| crate::model::Port {
                id: crate::ids::PortId::new(),
                name: p.name.clone(),
                row: p.row,
                col: p.col,
                kind: p.kind,
                gender: p.gender,
            })
            .collect(),
    }
}

fn model_ports(device: &Device) -> Vec<ModelPort> {
    device
        .ports
        .iter()
        .map(|p| ModelPort {
            name: p.name.clone(),
            row: p.row,
            col: p.col,
            kind: p.kind,
            gender: p.gender,
        })
        .collect()
}

fn model_index(catalog: &Catalog, model_id: ModelId) -> Result<usize, Rejection> {
    catalog
        .entries
        .iter()
        .position(|e| e.id == model_id)
        .ok_or(Rejection::NotFound)
}

/// The model must not be empty, and manufacturer + model must be unused (except by `except`).
fn check_identity(
    catalog: &Catalog,
    except: Option<ModelId>,
    manufacturer: &ModelText,
    model: &ModelText,
) -> Result<(), Rejection> {
    if model.is_empty() {
        return Err(Rejection::ModelRequired);
    }
    let name = display_name(manufacturer, model);
    let key = name.to_lowercase();
    if catalog
        .entries
        .iter()
        .any(|e| Some(e.id) != except && e.key() == key)
    {
        return Err(Rejection::ModelTaken(name));
    }
    Ok(())
}
