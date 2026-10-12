mod common;

use common::*;
use rackwright_core::edit::*;
use rackwright_core::file_format::{
    LoadError, catalog_from_json, catalog_to_json, from_json, load_station, to_json,
};
use rackwright_core::ids::{DeviceId, ModelId};
use rackwright_core::model::{Catalog, CatalogEntry, DeviceKind, Document, PortKind, Rgb};
use rackwright_core::name::ModelText;
use rackwright_core::validate::{ValidationError, validate, validate_catalog};

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
    c: &Catalog,
    device: DeviceId,
    manufacturer: &str,
    model: &str,
) -> Result<CatalogPlan, Rejection> {
    plan_save_model(d, c, device, text(manufacturer), text(model))
}

/// The station with its DMM saved as "Keysight 34465A".
fn saved() -> (Document, Catalog, DeviceId, ModelId) {
    let (d, dmm) = station();
    let plan = save(&d, &Catalog::default(), dmm, "Keysight", "34465A").unwrap();
    let id = plan.catalog.entries[0].id;
    (plan.document, plan.catalog, dmm, id)
}

fn place(d: &Document, c: &Catalog, id: ModelId, bottom_u: u32) -> Document {
    let entry = c.model(id).unwrap().clone();
    let r1 = d.racks[0].id;
    plan_device_drop(d, &limits(), &DeviceSource::Model(entry), r1, bottom_u)
        .unwrap()
        .document
}

fn kinds(d: &Document) -> Vec<DeviceKind> {
    d.racks
        .iter()
        .flat_map(|r| &r.devices)
        .map(|x| x.kind)
        .collect()
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
    let plan = save(&d, &Catalog::default(), dmm, "Keysight", "34465A").unwrap();
    assert_eq!(plan.catalog.entries.len(), 1);
    let entry = &plan.catalog.entries[0];
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
    let (_, device) = plan.document.device(dmm).unwrap();
    assert_eq!(device.kind, DeviceKind::Model(entry.id));
    assert_eq!(plan.catalog.model(entry.id).map(|e| e.id), Some(entry.id));
    assert_eq!(plan.subject, Some(ObjectId::Device(dmm)));
    assert_eq!(validate(&plan.document, &limits()), Ok(()));
    assert_eq!(validate_catalog(&plan.catalog, &limits()), Ok(()));
}

#[test]
fn manufacturer_and_model_are_unique_case_insensitively() {
    let (d, c, dmm, _) = saved();
    assert_eq!(
        save(&d, &c, dmm, "KEYSIGHT", "34465a").map(|_| ()),
        Err(Rejection::ModelTaken("KEYSIGHT 34465a".into()))
    );
    assert!(
        save(&d, &c, dmm, "", "34465A").is_ok(),
        "no manufacturer is different"
    );
    assert_eq!(
        save(&d, &c, dmm, "Keysight", " ").map(|_| ()),
        Err(Rejection::ModelRequired)
    );
}

#[test]
fn placing_a_model_copies_its_contents_into_a_new_linked_device() {
    let (d, c, dmm, id) = saved();
    let r1 = d.racks[0].id;
    let entry = c.model(id).unwrap().clone();
    let plan = plan_device_drop(&d, &limits(), &DeviceSource::Model(entry), r1, 20).unwrap();
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
    let (d, c, dmm, _) = saved();
    let plan = save(&d, &c, dmm, "", "Power Supply Unit 3000").unwrap();
    let (d, c) = (plan.document, plan.catalog);
    let long = c
        .entries
        .iter()
        .find(|e| e.model.as_str().starts_with("Power"))
        .unwrap()
        .id;
    let d = place(&d, &c, long, 20);
    let d = place(&d, &c, long, 30);
    let mut names: Vec<String> = d.racks[0]
        .devices
        .iter()
        .map(|x| x.name.to_string())
        .collect();
    names.sort();
    assert_eq!(names, ["DMM", "PowerSup_2", "PowerSuppl"]);
}

#[test]
fn changing_a_model_never_changes_placed_devices() {
    let (d, c, dmm, id) = saved();
    let d = place(&d, &c, id, 20);
    let before = d.racks.clone();
    let plan = plan_rename_model(&d, &c, id, text("Agilent"), text("34401A")).unwrap();
    assert_eq!(plan.document.racks, before);
    assert_eq!(plan.catalog.entries[0].display_name(), "Agilent 34401A");
    let c = plan.catalog;

    // Updating from a changed device replaces the entry only.
    let d = plan_set_device_height(&d, &limits(), dmm, 3)
        .unwrap()
        .document;
    let plan = plan_update_model(&d, &c, dmm).unwrap();
    assert_eq!(plan.catalog.entries[0].height_u, 3);
    assert_eq!(plan.catalog.entries[0].display_name(), "Agilent 34401A");
    assert_eq!(plan.document, d);
}

#[test]
fn only_devices_linked_to_a_present_model_can_update_it() {
    let (d, c, dmm, _) = saved();
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
        plan_update_model(&d, &c, x).map(|_| ()),
        Err(Rejection::NotLinked)
    );
    assert_eq!(
        plan_update_model(&d, &Catalog::default(), dmm).map(|_| ()),
        Err(Rejection::ModelMissing)
    );
}

#[test]
fn renaming_follows_the_uniqueness_rule() {
    let (d, c, dmm, first) = saved();
    let c = save(&d, &c, dmm, "Keysight", "E36313A").unwrap().catalog;
    assert_eq!(
        plan_rename_model(&d, &c, first, text("keysight"), text("e36313a")).map(|_| ()),
        Err(Rejection::ModelTaken("keysight e36313a".into()))
    );
    assert!(plan_rename_model(&d, &c, first, text("Keysight"), text("34465a")).is_ok());
    assert_eq!(
        plan_rename_model(&d, &c, ModelId::new(), text("A"), text("B")).map(|_| ()),
        Err(Rejection::NotFound)
    );
}

#[test]
fn deleting_a_model_unlinks_its_devices() {
    let (d, c, dmm, id) = saved();
    let plan = plan_remove_model(&d, &c, id).unwrap();
    assert!(plan.catalog.entries.is_empty());
    let (_, device) = plan.document.device(dmm).unwrap();
    assert_eq!(device.kind, DeviceKind::AdHoc);
    assert_eq!(device.ports, d.device(dmm).unwrap().1.ports);
    assert_eq!(validate(&plan.document, &limits()), Ok(()));
}

#[test]
fn stations_may_link_to_models_that_are_not_in_the_catalogue() {
    let (d, _, _, _) = saved();
    assert_eq!(validate(&d, &limits()), Ok(()));
    assert_eq!(from_json(&to_json(&d), &limits()), Ok(d));
}

#[test]
fn stations_are_saved_without_a_catalogue() {
    let (d, _, _, id) = saved();
    let json = to_json(&d);
    assert!(json.starts_with("{\n  \"format_version\": 6,"), "{json}");
    assert!(!json.contains("\"catalog\""), "{json}");
    assert!(json.contains(&format!("\"model\": \"{}\"", id.0)), "{json}");
}

#[test]
fn the_catalogue_survives_its_own_file() {
    let (_, c, _, _) = saved();
    let json = catalog_to_json(&c);
    assert!(
        json.starts_with("{\n  \"kind\": \"rackwright-catalog\",\n  \"format_version\": 1,"),
        "{json}"
    );
    assert!(json.contains("\"entries\": ["), "{json}");
    assert_eq!(catalog_from_json(&json, &limits()), Ok(c));
}

#[test]
fn station_and_catalogue_files_are_told_apart() {
    let (d, c, _, _) = saved();
    assert_eq!(
        catalog_from_json(&to_json(&d), &limits()),
        Err(LoadError::StationFile)
    );
    assert_eq!(
        from_json(&catalog_to_json(&c), &limits()),
        Err(LoadError::CatalogFile)
    );
    assert_eq!(
        catalog_from_json(r#"{"format_version": 1, "entries": []}"#, &limits()),
        Err(LoadError::NotCatalog)
    );
    let newer = catalog_to_json(&c).replacen("\"format_version\": 1", "\"format_version\": 2", 1);
    assert_eq!(
        catalog_from_json(&newer, &limits()),
        Err(LoadError::TooNew { found: 2 })
    );
}

/// A version-5 file: the station with its catalogue embedded.
fn version_5(d: &Document, c: &Catalog) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&to_json(d)).unwrap();
    let entries: serde_json::Value = serde_json::from_str(&catalog_to_json(c)).unwrap();
    value["format_version"] = 5.into();
    value["catalog"] = entries["entries"].clone();
    value.to_string()
}

#[test]
fn older_files_hand_back_their_embedded_catalogue() {
    let (d, c, _, _) = saved();
    let loaded = load_station(&version_5(&d, &c), &limits()).unwrap();
    assert_eq!(loaded.document, d);
    assert_eq!(loaded.legacy_catalog, Some(c));

    // Before version 4 there was no catalogue.
    let mut value: serde_json::Value = serde_json::from_str(&to_json(&d)).unwrap();
    value["format_version"] = 3.into();
    let loaded = load_station(&value.to_string(), &limits()).unwrap();
    assert_eq!(loaded.legacy_catalog, None);
    assert_eq!(loaded.document.racks, d.racks);
}

#[test]
fn an_invalid_embedded_catalogue_rejects_the_file() {
    let (d, mut c, _, _) = saved();
    c.entries[0].height_u = 0;
    assert!(matches!(
        load_station(&version_5(&d, &c), &limits()),
        Err(LoadError::Invalid(ValidationError::InvalidModel { .. }))
    ));
}

fn entry_named(c: &Catalog, display: &str) -> CatalogEntry {
    c.entries
        .iter()
        .find(|e| e.display_name() == display)
        .unwrap()
        .clone()
}

/// The saved station plus a second model, "Keysight E36313A", placed at U30.
fn two_models() -> (Document, Catalog) {
    let (d, c, _, _) = saved();
    let r1 = d.racks[0].id;
    let d = plan_device_drop(
        &d,
        &limits(),
        &DeviceSource::New {
            name: name("PSU"),
            height_u: 1,
        },
        r1,
        30,
    )
    .unwrap()
    .document;
    let psu = d.racks[0]
        .devices
        .iter()
        .find(|x| x.name.as_str() == "PSU")
        .unwrap()
        .id;
    let plan = save(&d, &c, psu, "Keysight", "E36313A").unwrap();
    (plan.document, plan.catalog)
}

#[test]
fn replacing_the_catalogue_keeps_relinks_or_unlinks_devices() {
    let (d, old) = two_models();
    let dmm = entry_named(&old, "Keysight 34465A");

    // The file has the DMM under the same ID, the PSU's model under a new ID (and new
    // case), and no third model.
    let mut psu = entry_named(&old, "Keysight E36313A");
    psu.id = ModelId::new();
    psu.model = text("e36313a");
    let incoming = Catalog {
        entries: vec![dmm.clone(), psu.clone()],
    };
    let plan = plan_replace_catalog(&d, &old, incoming.clone());
    assert_eq!(plan.catalog, incoming);
    assert_eq!(
        kinds(&plan.document),
        vec![DeviceKind::Model(dmm.id), DeviceKind::Model(psu.id)]
    );

    // Models that are missing from the file make their devices ad hoc.
    let plan = plan_replace_catalog(&d, &old, Catalog::default());
    assert_eq!(kinds(&plan.document), vec![DeviceKind::AdHoc; 2]);
}

#[test]
fn merging_keeps_existing_ids_and_relinks_the_incoming_station() {
    let (_, mine) = two_models();
    let (theirs_doc, theirs) = {
        // Another station saved the DMM's model again, under a fresh ID, with a new height.
        let (d, dmm) = station();
        let d = plan_set_device_height(&d, &limits(), dmm, 3)
            .unwrap()
            .document;
        let plan = save(&d, &Catalog::default(), dmm, "KEYSIGHT", "34465A").unwrap();
        let mut c = plan.catalog;
        let other = CatalogEntry {
            id: ModelId::new(),
            model: text("New model"),
            ..c.entries[0].clone()
        };
        c.entries.push(other);
        (plan.document, c)
    };
    let (merged, relinked) = merge_catalog(&mine, &theirs, &theirs_doc);
    assert_eq!(validate_catalog(&merged, &limits()), Ok(()));
    assert_eq!(merged.entries.len(), 3, "one overwritten, one added");

    let kept = entry_named(&mine, "Keysight 34465A").id;
    let overwritten = merged.model(kept).unwrap();
    assert_eq!(overwritten.display_name(), "KEYSIGHT 34465A");
    assert_eq!(overwritten.height_u, 3, "the incoming contents win");
    assert_eq!(kinds(&relinked), vec![DeviceKind::Model(kept)]);
    assert!(merged.by_key("keysight new model").is_some());

    // Merging the same catalogue again changes nothing.
    let (again, _) = merge_catalog(&merged, &theirs, &theirs_doc);
    assert_eq!(again, merged);
}

#[test]
fn merging_by_id_drops_an_entry_whose_text_it_takes_over() {
    let (_, mine) = two_models();
    let dmm = entry_named(&mine, "Keysight 34465A");
    let psu = entry_named(&mine, "Keysight E36313A");
    // The DMM's entry, renamed elsewhere to the PSU's text.
    let renamed = CatalogEntry {
        model: text("E36313A"),
        ..dmm.clone()
    };
    let incoming = Catalog {
        entries: vec![renamed],
    };
    let (merged, _) = merge_catalog(&mine, &incoming, &doc(vec![]));
    assert_eq!(merged.entries.len(), 1);
    assert_eq!(merged.entries[0].id, dmm.id);
    assert!(merged.model(psu.id).is_none());
    assert_eq!(validate_catalog(&merged, &limits()), Ok(()));
}

fn invalid(c: &Catalog) -> ValidationError {
    validate_catalog(c, &limits()).unwrap_err()
}

#[test]
fn invalid_catalogues_are_rejected() {
    let (_, c, _, _) = saved();

    let mut twice = c.clone();
    let mut copy: CatalogEntry = twice.entries[0].clone();
    copy.id = ModelId::new();
    copy.model = text("34465a");
    twice.entries.push(copy);
    assert_eq!(
        invalid(&twice),
        ValidationError::DuplicateModel("Keysight 34465a".into())
    );

    let mut outside = c.clone();
    outside.entries[0].ports[1].row = 2;
    assert!(matches!(
        invalid(&outside),
        ValidationError::InvalidModel { .. }
    ));
    assert_eq!(
        invalid(&outside).to_string(),
        "Model 'Keysight 34465A': port 'LO' is outside the device face"
    );

    let mut flat = c.clone();
    flat.entries[0].height_u = 0;
    assert!(matches!(
        invalid(&flat),
        ValidationError::InvalidModel { .. }
    ));

    let mut same_id = c.clone();
    let mut copy = same_id.entries[0].clone();
    copy.model = text("Other");
    same_id.entries.push(copy);
    assert!(matches!(invalid(&same_id), ValidationError::DuplicateId(_)));

    // Import goes through the same checks, and empty models are malformed text.
    let json = catalog_to_json(&c).replace("\"model\": \"34465A\"", "\"model\": \" \"");
    assert!(matches!(
        catalog_from_json(&json, &limits()),
        Err(LoadError::InvalidModelText { .. })
    ));
}
