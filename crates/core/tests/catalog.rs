mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::file_format::{LoadError, from_json, to_json};
use rackwright_core::ids::{DeviceId, ModelId};
use rackwright_core::model::{CatalogEntry, DeviceKind, Document, PortKind, Rgb};
use rackwright_core::name::ModelText;
use rackwright_core::validate::{ValidationError, validate};

fn text(s: &str) -> ModelText {
    ModelText::parse(s).unwrap()
}

/// R1 holds a 2U red DMM with HI (BNC) and LO.
fn station() -> (Document, DeviceId) {
    let mut dmm = with_ports(
        device("DMM", 10, 2),
        vec![port("HI", 0, 0), port("LO", 1, 3)],
    );
    dmm.color = Rgb { r: 200, g: 0, b: 0 };
    dmm.ports[0].kind = PortKind::Bnc;
    let id = dmm.id;
    (doc(vec![rack("R1", 42, vec![dmm])]), id)
}

fn save(
    d: &Document,
    device: DeviceId,
    manufacturer: &str,
    model: &str,
) -> Result<Plan, Rejection> {
    plan_save_model(d, device, text(manufacturer), text(model))
}

fn saved() -> (Document, DeviceId, ModelId) {
    let (d, dmm) = station();
    let d = save(&d, dmm, "Keysight", "34465A").unwrap().document;
    let id = d.catalog[0].id;
    (d, dmm, id)
}

#[test]
fn model_text_is_trimmed_and_checked() {
    assert_eq!(
        ModelText::parse("  Keysight  ").unwrap().as_str(),
        "Keysight"
    );
    assert_eq!(
        ModelText::parse("Rohde & Schwarz").unwrap().as_str(),
        "Rohde & Schwarz"
    );
    assert!(ModelText::parse("   ").unwrap().is_empty());
    assert!(ModelText::parse(&"x".repeat(41)).is_err());
    assert!(ModelText::parse("a\tb").is_err());
    assert_eq!(
        ModelText::parse(&"x".repeat(41)).unwrap_err().to_string(),
        "text must be at most 40 characters"
    );
}

#[test]
fn saving_a_device_as_a_model_copies_it_and_links_it() {
    let (d, dmm) = station();
    let plan = save(&d, dmm, "Keysight", "34465A").unwrap();
    let doc = &plan.document;
    assert_eq!(doc.catalog.len(), 1);
    let entry = &doc.catalog[0];
    assert_eq!(entry.display_name(), "Keysight 34465A");
    assert_eq!(entry.height_u, 2);
    assert_eq!(entry.color, Rgb { r: 200, g: 0, b: 0 });
    let ports: Vec<(String, u32, u32, PortKind)> = entry
        .ports
        .iter()
        .map(|p| (p.name.to_string(), p.row, p.col, p.kind))
        .collect();
    assert_eq!(
        ports,
        vec![
            ("HI".into(), 0, 0, PortKind::Bnc),
            ("LO".into(), 1, 3, PortKind::Unspecified)
        ]
    );
    let (_, device) = doc.device(dmm).unwrap();
    assert_eq!(device.kind, DeviceKind::Model(entry.id));
    assert_eq!(doc.model(entry.id).map(|e| e.id), Some(entry.id));
    assert_eq!(plan.subject, Some(ObjectId::Device(dmm)));
    assert_eq!(validate(doc, &limits()), Ok(()));
}

#[test]
fn manufacturer_and_model_are_unique_case_insensitively() {
    let (d, dmm, _) = saved();
    assert_eq!(
        save(&d, dmm, "KEYSIGHT", "34465a").map(|_| ()),
        Err(Rejection::ModelTaken("KEYSIGHT 34465a".into()))
    );
    assert!(
        save(&d, dmm, "", "34465A").is_ok(),
        "no manufacturer is different"
    );
    assert_eq!(
        save(&d, dmm, "Keysight", " ").map(|_| ()),
        Err(Rejection::ModelRequired)
    );
}

#[test]
fn placing_a_model_copies_its_contents_into_a_new_linked_device() {
    let (d, dmm, id) = saved();
    let r1 = d.racks[0].id;
    let plan = plan_device_drop(&d, &limits(), &DeviceSource::Model(id), r1, 20).unwrap();
    let Some(ObjectId::Device(new)) = plan.subject else {
        panic!("a device was created")
    };
    let (_, placed) = plan.document.device(new).unwrap();
    let (_, original) = d.device(dmm).unwrap();
    assert_eq!(placed.kind, DeviceKind::Model(id));
    assert_eq!((placed.bottom_u, placed.height_u), (20, 2));
    assert_eq!(placed.color, original.color);
    assert_eq!(placed.name.as_str(), "34465A");
    let cells = |dev: &rackwright_core::model::Device| {
        dev.ports
            .iter()
            .map(|p| (p.name.to_string(), p.row, p.col, p.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(placed), cells(original));
    assert!(
        placed
            .ports
            .iter()
            .all(|p| original.ports.iter().all(|q| q.id != p.id))
    );
    assert_eq!(validate(&plan.document, &limits()), Ok(()));
}

#[test]
fn placed_names_come_from_the_model_text_and_avoid_clashes() {
    let (d, dmm, _) = saved();
    let d = save(&d, dmm, "", "Power Supply Unit 3000")
        .unwrap()
        .document;
    let long = d
        .catalog
        .iter()
        .find(|e| e.model.as_str().starts_with("Power"))
        .unwrap()
        .id;
    let r1 = d.racks[0].id;
    let d = plan_device_drop(&d, &limits(), &DeviceSource::Model(long), r1, 20)
        .unwrap()
        .document;
    let d = plan_device_drop(&d, &limits(), &DeviceSource::Model(long), r1, 30)
        .unwrap()
        .document;
    let mut names: Vec<String> = d.racks[0]
        .devices
        .iter()
        .map(|x| x.name.to_string())
        .collect();
    names.sort();
    assert_eq!(names, ["DMM", "PowerSup_2", "PowerSuppl"]);
}

#[test]
fn placing_a_missing_model_is_rejected() {
    let (d, _) = station();
    let r1 = d.racks[0].id;
    assert_eq!(
        plan_device_drop(&d, &limits(), &DeviceSource::Model(ModelId::new()), r1, 1).map(|_| ()),
        Err(Rejection::NotFound)
    );
}

#[test]
fn changing_a_model_never_changes_placed_devices() {
    let (d, dmm, id) = saved();
    let r1 = d.racks[0].id;
    let d = plan_device_drop(&d, &limits(), &DeviceSource::Model(id), r1, 20)
        .unwrap()
        .document;
    let before = d.racks.clone();
    let d = plan_rename_model(&d, id, text("Agilent"), text("34401A"))
        .unwrap()
        .document;
    assert_eq!(d.racks, before);
    assert_eq!(d.catalog[0].display_name(), "Agilent 34401A");

    // Updating from a changed device replaces the entry only.
    let d = plan_set_device_height(&d, &limits(), dmm, 3)
        .unwrap()
        .document;
    let plan = plan_update_model(&d, dmm).unwrap();
    assert_eq!(plan.document.catalog[0].height_u, 3);
    assert_eq!(plan.document.catalog[0].display_name(), "Agilent 34401A");
    assert_eq!(plan.document.racks, d.racks);
}

#[test]
fn only_linked_devices_can_update_their_model() {
    let (d, _, _) = saved();
    let r1 = d.racks[0].id;
    let d = plan_device_drop(
        &d,
        &limits(),
        &DeviceSource::New {
            name: name("X"),
            height_u: 1,
        },
        r1,
        30,
    )
    .unwrap()
    .document;
    let x = d.racks[0]
        .devices
        .iter()
        .find(|x| x.name.as_str() == "X")
        .unwrap()
        .id;
    assert_eq!(
        plan_update_model(&d, x).map(|_| ()),
        Err(Rejection::NotLinked)
    );
}

#[test]
fn renaming_follows_the_uniqueness_rule() {
    let (d, dmm, first) = saved();
    let d = save(&d, dmm, "Keysight", "E36313A").unwrap().document;
    assert_eq!(
        plan_rename_model(&d, first, text("keysight"), text("e36313a")).map(|_| ()),
        Err(Rejection::ModelTaken("keysight e36313a".into()))
    );
    assert!(plan_rename_model(&d, first, text("Keysight"), text("34465a")).is_ok());
    assert_eq!(
        plan_rename_model(&d, ModelId::new(), text("A"), text("B")).map(|_| ()),
        Err(Rejection::NotFound)
    );
}

#[test]
fn deleting_a_model_unlinks_its_devices() {
    let (d, dmm, id) = saved();
    let plan = plan_remove_model(&d, id).unwrap();
    assert!(plan.document.catalog.is_empty());
    let (_, device) = plan.document.device(dmm).unwrap();
    assert_eq!(device.kind, DeviceKind::AdHoc);
    assert_eq!(device.ports, d.device(dmm).unwrap().1.ports);
    assert_eq!(validate(&plan.document, &limits()), Ok(()));
}

#[test]
fn the_catalogue_survives_a_file_round_trip() {
    let (d, _, _) = saved();
    let json = to_json(&d);
    assert!(json.starts_with("{\n  \"format_version\": 5,"), "{json}");
    assert!(json.contains("\"catalog\": ["), "{json}");
    assert!(json.contains("\"model\": \""), "{json}");
    assert_eq!(from_json(&json, &limits()), Ok(d));
}

#[test]
fn older_files_load_with_an_empty_catalogue() {
    let (d, _) = station();
    let mut value: serde_json::Value = serde_json::from_str(&to_json(&d)).unwrap();
    value["format_version"] = 3.into();
    value.as_object_mut().unwrap().remove("catalog");
    let loaded = from_json(&value.to_string(), &limits()).unwrap();
    assert!(loaded.catalog.is_empty());
    assert_eq!(loaded.racks, d.racks);
}

fn invalid(d: &Document) -> ValidationError {
    validate(d, &limits()).unwrap_err()
}

#[test]
fn invalid_catalogues_are_rejected() {
    let (d, dmm, _) = saved();

    let mut dangling = d.clone();
    dangling.catalog.clear();
    assert_eq!(
        invalid(&dangling),
        ValidationError::DeviceModelMissing {
            device: "DMM".into()
        }
    );

    let mut twice = d.clone();
    let mut copy: CatalogEntry = twice.catalog[0].clone();
    copy.id = ModelId::new();
    copy.model = text("34465a");
    twice.catalog.push(copy);
    assert_eq!(
        invalid(&twice),
        ValidationError::DuplicateModel("Keysight 34465a".into())
    );

    let mut outside = d.clone();
    outside.catalog[0].ports[1].row = 2;
    assert!(matches!(
        invalid(&outside),
        ValidationError::InvalidModel { .. }
    ));
    assert_eq!(
        invalid(&outside).to_string(),
        "Model 'Keysight 34465A': port 'LO' is outside the device face"
    );

    let mut flat = d.clone();
    flat.catalog[0].height_u = 0;
    assert!(matches!(
        invalid(&flat),
        ValidationError::InvalidModel { .. }
    ));

    let mut same_id = d.clone();
    same_id.catalog[0].id = ModelId(dmm.0);
    same_id.racks[0].devices[0].kind = DeviceKind::Model(ModelId(dmm.0));
    assert!(matches!(invalid(&same_id), ValidationError::DuplicateId(_)));

    // Import goes through the same checks, and empty models are malformed text.
    let json = to_json(&d).replace("\"model\": \"34465A\"", "\"model\": \" \"");
    assert!(matches!(
        from_json(&json, &limits()),
        Err(LoadError::InvalidModelText { .. })
    ));
}
