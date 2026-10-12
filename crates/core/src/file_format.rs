//! Versioned JSON file formats (spec §4.1, §5): station files and catalogue files, each with
//! its own version. The `File*` types are the on-disk shape and are kept separate from the
//! in-memory model so the model can change without breaking files.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::ids::{CableId, DeviceId, ModelId, PortId, RackId};
use crate::limits::Limits;
use crate::model::{
    Cable, Catalog, CatalogEntry, Device, DeviceKind, Document, Gender, ModelPort, Port, PortKind,
    Rack, Rgb,
};
use crate::name::{DocumentName, ModelText, Name};
use crate::validate::{ValidationError, validate, validate_catalog};

/// The station file version.
pub const FORMAT_VERSION: u64 = 6;

/// The catalogue file version (independent of `FORMAT_VERSION`).
pub const CATALOG_FORMAT_VERSION: u64 = 1;

/// The `kind` that marks a catalogue file.
pub const CATALOG_KIND: &str = "rackwright-catalog";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("The file is not valid JSON: {0}")]
    NotJson(String),
    #[error("The file has no format version.")]
    MissingVersion,
    #[error("This file was made by a newer version of the app.")]
    TooNew { found: u64 },
    #[error("The file is malformed: {0}")]
    Malformed(String),
    #[error("Invalid name '{name}': {reason}")]
    InvalidName { name: String, reason: String },
    #[error("Invalid colour '{0}'")]
    InvalidColor(String),
    #[error("Invalid catalogue text '{text}': {reason}")]
    InvalidModelText { text: String, reason: String },
    #[error("This is a catalogue file. Use Import in the Catalogue panel.")]
    CatalogFile,
    #[error("This is a station file, not a catalogue.")]
    StationFile,
    #[error("The file is not a catalogue.")]
    NotCatalog,
    #[error(transparent)]
    Invalid(#[from] ValidationError),
}

/// A loaded station file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedStation {
    pub document: Document,
    /// The catalogue that format versions 4 and 5 embedded in the station, if the file had one.
    pub legacy_catalog: Option<Catalog>,
}

#[derive(Serialize, Deserialize)]
struct FileDocument {
    format_version: u64,
    name: String,
    racks: Vec<FileRack>,
    cables: Vec<FileCable>,
}

#[derive(Serialize, Deserialize)]
struct FileCatalog {
    kind: String,
    format_version: u64,
    entries: Vec<FileCatalogEntry>,
}

#[derive(Serialize, Deserialize)]
struct FileCatalogEntry {
    id: Uuid,
    manufacturer: String,
    model: String,
    height_u: u32,
    color: String,
    ports: Vec<FileModelPort>,
}

#[derive(Serialize, Deserialize)]
struct FileModelPort {
    name: String,
    row: u32,
    col: u32,
    #[serde(default)]
    kind: FilePortKind,
    #[serde(default)]
    gender: FileGender,
}

#[derive(Serialize, Deserialize)]
struct FileRack {
    id: Uuid,
    name: String,
    height_u: u32,
    devices: Vec<FileDevice>,
}

#[derive(Serialize, Deserialize)]
struct FileDevice {
    id: Uuid,
    name: String,
    bottom_u: u32,
    height_u: u32,
    color: String,
    #[serde(default)]
    kind: FileDeviceKind,
    ports: Vec<FilePort>,
}

#[derive(Serialize, Deserialize)]
struct FilePort {
    id: Uuid,
    name: String,
    row: u32,
    col: u32,
    #[serde(default)]
    kind: FilePortKind,
    #[serde(default)]
    gender: FileGender,
}

#[derive(Serialize, Deserialize)]
struct FileCable {
    id: Uuid,
    name: String,
    color: String,
    a: Uuid,
    b: Uuid,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum FileDeviceKind {
    #[default]
    AdHoc,
    /// Written as `{ "model": "<uuid>" }`.
    Model(Uuid),
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum FileGender {
    #[default]
    Unspecified,
    Male,
    Female,
    Other,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum FilePortKind {
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

pub fn to_json(doc: &Document) -> String {
    let file = FileDocument {
        format_version: FORMAT_VERSION,
        name: doc.name.as_str().to_owned(),
        racks: doc.racks.iter().map(rack_to_file).collect(),
        cables: doc.cables.iter().map(cable_to_file).collect(),
    };
    serde_json::to_string_pretty(&file).expect("serializing plain data cannot fail")
}

/// Loads a station file, discarding a legacy embedded catalogue (see `load_station`).
pub fn from_json(text: &str, limits: &Limits) -> Result<Document, LoadError> {
    load_station(text, limits).map(|loaded| loaded.document)
}

/// Loads a station file, returning the catalogue an older file embedded alongside it.
pub fn load_station(text: &str, limits: &Limits) -> Result<LoadedStation, LoadError> {
    let mut value = parse(text)?;
    if is_catalog(&value) {
        return Err(LoadError::CatalogFile);
    }
    let version = version_of(&value)?;
    if version > FORMAT_VERSION {
        return Err(LoadError::TooNew { found: version });
    }
    // Versions 4 and 5 embedded the catalogue; it now lives in its own file.
    let legacy = match value.as_object_mut() {
        Some(object) if (4..6).contains(&version) => object.remove("catalog"),
        _ => None,
    };
    let value = migrate(value, version)?;
    let file: FileDocument =
        serde_json::from_value(value).map_err(|e| LoadError::Malformed(e.to_string()))?;
    let document = document_from_file(file, limits)?;
    validate(&document, limits)?;
    let legacy_catalog = legacy
        .map(|value| {
            let entries: Vec<FileCatalogEntry> =
                serde_json::from_value(value).map_err(|e| LoadError::Malformed(e.to_string()))?;
            catalog_from_entries(entries, limits)
        })
        .transpose()?;
    Ok(LoadedStation {
        document,
        legacy_catalog,
    })
}

pub fn catalog_to_json(catalog: &Catalog) -> String {
    let file = FileCatalog {
        kind: CATALOG_KIND.to_owned(),
        format_version: CATALOG_FORMAT_VERSION,
        entries: catalog.entries.iter().map(entry_to_file).collect(),
    };
    serde_json::to_string_pretty(&file).expect("serializing plain data cannot fail")
}

pub fn catalog_from_json(text: &str, limits: &Limits) -> Result<Catalog, LoadError> {
    let value = parse(text)?;
    if !is_catalog(&value) {
        return Err(if value.get("racks").is_some() {
            LoadError::StationFile
        } else {
            LoadError::NotCatalog
        });
    }
    let version = version_of(&value)?;
    if version > CATALOG_FORMAT_VERSION {
        return Err(LoadError::TooNew { found: version });
    }
    if version != 1 {
        return Err(LoadError::Malformed(format!(
            "unsupported catalogue version {version}"
        )));
    }
    let file: FileCatalog =
        serde_json::from_value(value).map_err(|e| LoadError::Malformed(e.to_string()))?;
    catalog_from_entries(file.entries, limits)
}

fn catalog_from_entries(
    entries: Vec<FileCatalogEntry>,
    limits: &Limits,
) -> Result<Catalog, LoadError> {
    let catalog = Catalog {
        entries: entries
            .into_iter()
            .map(|e| entry_from_file(e, limits))
            .collect::<Result<Vec<_>, _>>()?,
    };
    validate_catalog(&catalog, limits)?;
    Ok(catalog)
}

fn parse(text: &str) -> Result<Value, LoadError> {
    serde_json::from_str(text).map_err(|e| LoadError::NotJson(e.to_string()))
}

fn version_of(value: &Value) -> Result<u64, LoadError> {
    value
        .get("format_version")
        .and_then(Value::as_u64)
        .ok_or(LoadError::MissingVersion)
}

fn is_catalog(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some(CATALOG_KIND)
}

/// Upgrades older station versions step by step to `FORMAT_VERSION`.
fn migrate(mut value: Value, version: u64) -> Result<Value, LoadError> {
    match version {
        // Version 2 added cables.
        1 => {
            if let Some(object) = value.as_object_mut() {
                object.insert("cables".into(), Value::Array(Vec::new()));
            }
            migrate(value, 2)
        }
        // Version 3 added port types; the shape is unchanged.
        2 => migrate(value, 3),
        // Version 4 added the embedded catalogue (`load_station` takes it out before this).
        3 => migrate(value, 4),
        // Version 5 added port genders; a missing gender reads as unspecified.
        4 => migrate(value, 5),
        // Version 6 moved the catalogue to its own file.
        5 => {
            if let Some(object) = value.as_object_mut() {
                object.remove("catalog");
            }
            migrate(value, 6)
        }
        6 => Ok(value),
        other => Err(LoadError::Malformed(format!(
            "unsupported format version {other}"
        ))),
    }
}

fn rack_to_file(rack: &Rack) -> FileRack {
    FileRack {
        id: rack.id.0,
        name: rack.name.as_str().to_owned(),
        height_u: rack.height_u,
        devices: rack.devices.iter().map(device_to_file).collect(),
    }
}

fn device_to_file(device: &Device) -> FileDevice {
    FileDevice {
        id: device.id.0,
        name: device.name.as_str().to_owned(),
        bottom_u: device.bottom_u,
        height_u: device.height_u,
        color: device.color.to_hex(),
        kind: match device.kind {
            DeviceKind::AdHoc => FileDeviceKind::AdHoc,
            DeviceKind::Model(id) => FileDeviceKind::Model(id.0),
        },
        ports: device.ports.iter().map(port_to_file).collect(),
    }
}

fn port_to_file(port: &Port) -> FilePort {
    FilePort {
        id: port.id.0,
        name: port.name.as_str().to_owned(),
        row: port.row,
        col: port.col,
        kind: kind_to_file(port.kind),
        gender: gender_to_file(port.gender),
    }
}

fn cable_to_file(cable: &Cable) -> FileCable {
    FileCable {
        id: cable.id.0,
        name: cable.name.as_str().to_owned(),
        color: cable.color.to_hex(),
        a: cable.a.0,
        b: cable.b.0,
    }
}

fn document_from_file(file: FileDocument, limits: &Limits) -> Result<Document, LoadError> {
    let name = DocumentName::parse(&file.name).map_err(|e| LoadError::InvalidName {
        name: file.name.clone(),
        reason: e.to_string(),
    })?;
    if name.as_str() != file.name {
        return Err(LoadError::InvalidName {
            name: file.name,
            reason: "has leading or trailing whitespace".into(),
        });
    }
    let racks = file
        .racks
        .into_iter()
        .map(|r| rack_from_file(r, limits))
        .collect::<Result<Vec<_>, _>>()?;
    let cables = file
        .cables
        .into_iter()
        .map(|c| cable_from_file(c, limits))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Document {
        name,
        racks,
        cables,
    })
}

fn entry_to_file(entry: &CatalogEntry) -> FileCatalogEntry {
    FileCatalogEntry {
        id: entry.id.0,
        manufacturer: entry.manufacturer.to_string(),
        model: entry.model.to_string(),
        height_u: entry.height_u,
        color: entry.color.to_hex(),
        ports: entry
            .ports
            .iter()
            .map(|p| FileModelPort {
                name: p.name.as_str().to_owned(),
                row: p.row,
                col: p.col,
                kind: kind_to_file(p.kind),
                gender: gender_to_file(p.gender),
            })
            .collect(),
    }
}

fn entry_from_file(entry: FileCatalogEntry, limits: &Limits) -> Result<CatalogEntry, LoadError> {
    let model = strict_model_text(&entry.model)?;
    if model.is_empty() {
        return Err(LoadError::InvalidModelText {
            text: entry.model,
            reason: "the model must not be empty".into(),
        });
    }
    Ok(CatalogEntry {
        id: ModelId(entry.id),
        manufacturer: strict_model_text(&entry.manufacturer)?,
        model,
        height_u: entry.height_u,
        color: Rgb::from_hex(&entry.color).ok_or(LoadError::InvalidColor(entry.color))?,
        ports: entry
            .ports
            .into_iter()
            .map(|p| {
                Ok(ModelPort {
                    name: strict_name(&p.name, limits)?,
                    row: p.row,
                    col: p.col,
                    kind: kind_from_file(p.kind),
                    gender: gender_from_file(p.gender),
                })
            })
            .collect::<Result<Vec<_>, LoadError>>()?,
    })
}

/// Like `ModelText::parse`, but rejects text that would need trimming.
fn strict_model_text(raw: &str) -> Result<ModelText, LoadError> {
    let invalid = |reason: String| LoadError::InvalidModelText {
        text: raw.to_owned(),
        reason,
    };
    let text = ModelText::parse(raw).map_err(|e| invalid(e.to_string()))?;
    if text.as_str() != raw {
        return Err(invalid("has leading or trailing whitespace".into()));
    }
    Ok(text)
}

fn gender_to_file(gender: Gender) -> FileGender {
    match gender {
        Gender::Unspecified => FileGender::Unspecified,
        Gender::Male => FileGender::Male,
        Gender::Female => FileGender::Female,
        Gender::Other => FileGender::Other,
    }
}

fn gender_from_file(gender: FileGender) -> Gender {
    match gender {
        FileGender::Unspecified => Gender::Unspecified,
        FileGender::Male => Gender::Male,
        FileGender::Female => Gender::Female,
        FileGender::Other => Gender::Other,
    }
}

fn kind_to_file(kind: PortKind) -> FilePortKind {
    match kind {
        PortKind::Unspecified => FilePortKind::Unspecified,
        PortKind::Bnc => FilePortKind::Bnc,
        PortKind::Sma => FilePortKind::Sma,
        PortKind::NType => FilePortKind::NType,
        PortKind::Banana => FilePortKind::Banana,
        PortKind::Usb => FilePortKind::Usb,
        PortKind::Lan => FilePortKind::Lan,
        PortKind::Gpib => FilePortKind::Gpib,
        PortKind::DSub => FilePortKind::DSub,
        PortKind::Power => FilePortKind::Power,
        PortKind::Other => FilePortKind::Other,
    }
}

fn kind_from_file(kind: FilePortKind) -> PortKind {
    match kind {
        FilePortKind::Unspecified => PortKind::Unspecified,
        FilePortKind::Bnc => PortKind::Bnc,
        FilePortKind::Sma => PortKind::Sma,
        FilePortKind::NType => PortKind::NType,
        FilePortKind::Banana => PortKind::Banana,
        FilePortKind::Usb => PortKind::Usb,
        FilePortKind::Lan => PortKind::Lan,
        FilePortKind::Gpib => PortKind::Gpib,
        FilePortKind::DSub => PortKind::DSub,
        FilePortKind::Power => PortKind::Power,
        FilePortKind::Other => PortKind::Other,
    }
}

fn cable_from_file(cable: FileCable, limits: &Limits) -> Result<Cable, LoadError> {
    Ok(Cable {
        id: CableId(cable.id),
        name: strict_name(&cable.name, limits)?,
        color: Rgb::from_hex(&cable.color).ok_or(LoadError::InvalidColor(cable.color))?,
        a: PortId(cable.a),
        b: PortId(cable.b),
    })
}

fn rack_from_file(rack: FileRack, limits: &Limits) -> Result<Rack, LoadError> {
    Ok(Rack {
        id: RackId(rack.id),
        name: strict_name(&rack.name, limits)?,
        height_u: rack.height_u,
        devices: rack
            .devices
            .into_iter()
            .map(|d| device_from_file(d, limits))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn device_from_file(device: FileDevice, limits: &Limits) -> Result<Device, LoadError> {
    Ok(Device {
        id: DeviceId(device.id),
        name: strict_name(&device.name, limits)?,
        bottom_u: device.bottom_u,
        height_u: device.height_u,
        color: Rgb::from_hex(&device.color).ok_or(LoadError::InvalidColor(device.color))?,
        kind: match device.kind {
            FileDeviceKind::AdHoc => DeviceKind::AdHoc,
            FileDeviceKind::Model(id) => DeviceKind::Model(ModelId(id)),
        },
        ports: device
            .ports
            .into_iter()
            .map(|p| port_from_file(p, limits))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn port_from_file(port: FilePort, limits: &Limits) -> Result<Port, LoadError> {
    Ok(Port {
        id: PortId(port.id),
        name: strict_name(&port.name, limits)?,
        row: port.row,
        col: port.col,
        kind: kind_from_file(port.kind),
        gender: gender_from_file(port.gender),
    })
}

/// Like `Name::parse`, but rejects names that would need normalising: files are never
/// silently corrected.
fn strict_name(raw: &str, limits: &Limits) -> Result<Name, LoadError> {
    let name = Name::parse(raw, limits).map_err(|e| LoadError::InvalidName {
        name: raw.to_owned(),
        reason: e.to_string(),
    })?;
    if name.as_str() != raw {
        return Err(LoadError::InvalidName {
            name: raw.to_owned(),
            reason: "contains whitespace".into(),
        });
    }
    Ok(name)
}
