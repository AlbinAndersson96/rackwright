//! Parsing and validating what the user types (spec §4.3 "Properties panel", "New … form").
//! Errors are short sentences shown inline next to the field.

use rackwright_core::ids::DeviceId;
use rackwright_core::limits::Limits;
use rackwright_core::model::{Document, Rgb};
use rackwright_core::name::{DocumentName, ModelText, Name};

pub fn parse_name(text: &str, limits: &Limits) -> Result<Name, String> {
    Name::parse(text, limits).map_err(|e| capitalise(&e.to_string()))
}

pub fn parse_document_name(text: &str) -> Result<DocumentName, String> {
    DocumentName::parse(text).map_err(|e| capitalise(&e.to_string()))
}

/// A catalogue manufacturer or model (emptiness is checked by the edit).
pub fn parse_model_text(text: &str) -> Result<ModelText, String> {
    ModelText::parse(text).map_err(|e| capitalise(&e.to_string()))
}

/// A height in whole rack units, at least 1.
pub fn parse_height(text: &str) -> Result<u32, String> {
    match text.trim().parse::<u32>() {
        Ok(h) if h >= 1 => Ok(h),
        _ => Err("Height must be a whole number of at least 1".into()),
    }
}

/// The new-device form: a valid name and height enable the drag handle.
pub fn new_device_input(name: &str, height: &str, limits: &Limits) -> Result<(Name, u32), String> {
    Ok((parse_name(name, limits)?, parse_height(height)?))
}

/// The new-port form: the name must be valid and unused on the target device.
pub fn new_port_input(
    doc: &Document,
    device: DeviceId,
    name: &str,
    limits: &Limits,
) -> Result<Name, String> {
    let name = parse_name(name, limits)?;
    let (_, d) = doc.device(device).ok_or("The device no longer exists")?;
    if d.ports.iter().any(|p| p.name.same_as(&name)) {
        return Err(format!("The name '{name}' is already used on this device"));
    }
    Ok(name)
}

/// `#rrggbb` for the native colour input.
pub fn color_to_input(c: Rgb) -> String {
    c.to_hex()
}

pub fn color_from_input(text: &str) -> Option<Rgb> {
    Rgb::from_hex(text)
}

pub fn export_file_name(doc: &Document) -> String {
    format!("{}.json", doc.name)
}

pub(crate) fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
