//! The tree: Document → Rack → Device → Port (spec §4.3 "Tree"), then the cables.

use leptos::prelude::*;
use rackwright_core::edit::ObjectId;

use crate::ui::core::{Menu, MenuOrigin, RenameTarget, now_s, read, signals, update};

/// One row of the tree, flattened for rendering.
#[derive(Clone, PartialEq)]
struct Row {
    target: RenameTarget,
    label: String,
    detail: String,
    depth: usize,
    rack_index: Option<usize>,
    /// `Some(expanded)` for rows that have children.
    expander: Option<bool>,
}

/// DOM id of an object's tree row (for scrolling it into view).
fn row_id(id: ObjectId) -> String {
    match id {
        ObjectId::Rack(r) => format!("row-{}", r.0),
        ObjectId::Device(d) => format!("row-{}", d.0),
        ObjectId::Port(p) => format!("row-{}", p.0),
        ObjectId::Cable(c) => format!("row-{}", c.0),
    }
}

/// The rack and device that contain `id` (the rows that must be expanded to show it).
fn parents(id: ObjectId) -> Vec<ObjectId> {
    read(|s| match id {
        ObjectId::Rack(_) | ObjectId::Cable(_) => vec![],
        ObjectId::Device(d) => s
            .document()
            .device(d)
            .map_or(vec![], |(r, _)| vec![ObjectId::Rack(r.id)]),
        ObjectId::Port(p) => s.document().port(p).map_or(vec![], |(r, d, _)| {
            vec![ObjectId::Rack(r.id), ObjectId::Device(d.id)]
        }),
    })
}

fn rows(collapsed: &[ObjectId]) -> Vec<Row> {
    read(|s| {
        let doc = s.document();
        let mut rows = vec![Row {
            target: RenameTarget::Document,
            label: doc.name.to_string(),
            detail: String::new(),
            depth: 0,
            rack_index: None,
            expander: None,
        }];
        for (ri, rack) in doc.racks.iter().enumerate() {
            let rack_id = ObjectId::Rack(rack.id);
            let rack_open = !collapsed.contains(&rack_id);
            rows.push(Row {
                target: RenameTarget::Object(rack_id),
                label: rack.name.to_string(),
                detail: format!("{}U", rack.height_u),
                depth: 1,
                rack_index: Some(ri),
                expander: (!rack.devices.is_empty()).then_some(rack_open),
            });
            if !rack_open {
                continue;
            }
            for device in rack.devices_top_down() {
                let device_id = ObjectId::Device(device.id);
                let device_open = !collapsed.contains(&device_id);
                rows.push(Row {
                    target: RenameTarget::Object(device_id),
                    label: device.name.to_string(),
                    detail: match device.kind {
                        rackwright_core::model::DeviceKind::Model(m) => doc.model(m).map_or_else(
                            || format!("U{}", device.bottom_u),
                            |e| format!("U{} · {}", device.bottom_u, e.model),
                        ),
                        rackwright_core::model::DeviceKind::AdHoc => {
                            format!("U{}", device.bottom_u)
                        }
                    },
                    depth: 2,
                    rack_index: None,
                    expander: (!device.ports.is_empty()).then_some(device_open),
                });
                if !device_open {
                    continue;
                }
                for port in device.ports_in_reading_order() {
                    rows.push(Row {
                        target: RenameTarget::Object(ObjectId::Port(port.id)),
                        label: port.name.to_string(),
                        detail: [
                            port_description(port),
                            doc.cable_at_port(port.id).map(|c| c.name.to_string()),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · "),
                        depth: 3,
                        rack_index: None,
                        expander: None,
                    });
                }
            }
        }
        rows
    })
}

/// "BNC male", "BNC", "Female" or nothing: a port's type and gender, unspecified parts left out.
fn port_description(port: &rackwright_core::model::Port) -> Option<String> {
    use rackwright_core::model::{Gender, PortKind};
    let kind = (port.kind != PortKind::Unspecified).then(|| port.kind.label().to_string());
    let gender = match port.gender {
        Gender::Unspecified => None,
        Gender::Male => Some("male"),
        Gender::Female => Some("female"),
        Gender::Other => Some("genderless"),
    };
    match (kind, gender) {
        (Some(k), Some(g)) => Some(format!("{k} {g}")),
        (Some(k), None) => Some(k),
        (None, Some(g)) => Some(crate::forms::capitalise(g)),
        (None, None) => None,
    }
}

/// One row per cable: its name, and its ends as `Device.Port – Device.Port`.
fn cable_rows() -> Vec<Row> {
    read(|s| {
        let doc = s.document();
        let end = |port| {
            doc.port(port).map_or("—".to_string(), |(_, d, p)| {
                format!("{}.{}", d.name, p.name)
            })
        };
        doc.cables
            .iter()
            .map(|c| Row {
                target: RenameTarget::Object(ObjectId::Cable(c.id)),
                label: c.name.to_string(),
                detail: format!(
                    "{}{} – {}",
                    if doc.cable_mismatch(c).is_some() {
                        "⚠ "
                    } else {
                        ""
                    },
                    end(c.a),
                    end(c.b)
                ),
                depth: 1,
                rack_index: None,
                expander: None,
            })
            .collect()
    })
}

#[component]
pub fn Tree() -> impl IntoView {
    let sig = signals();
    let collapsed = RwSignal::new(Vec::<ObjectId>::new());
    // Selecting something (e.g. in 3D) expands its parents and scrolls its row into view.
    Effect::new(move |_| {
        sig.rev.track();
        let Some(selection) = read(|s| s.selection()) else {
            return;
        };
        let hidden: Vec<ObjectId> = parents(selection)
            .into_iter()
            .filter(|p| collapsed.get_untracked().contains(p))
            .collect();
        if !hidden.is_empty() {
            collapsed.update(|c| c.retain(|id| !hidden.contains(id)));
        }
        let id = row_id(selection);
        leptos::task::spawn_local(async move {
            if let Some(row) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id(&id))
            {
                row.scroll_into_view_with_bool(false);
            }
        });
    });
    view! {
        <ul class="tree">
            {move || {
                sig.rev.track();
                let selection = read(|s| s.selection());
                rows(&collapsed.get())
                    .into_iter()
                    .map(|row| {
                        let selected = matches!(row.target, RenameTarget::Object(id) if Some(id) == selection);
                        view! { <TreeRow row=row selected=selected collapsed=collapsed /> }
                    })
                    .collect_view()
            }}
        </ul>
        <button class="add-rack" on:click=move |_| update(|s| s.add_rack())>"Add rack"</button>
        {move || {
            sig.rev.track();
            let rows = cable_rows();
            (!rows.is_empty()).then(|| {
                let selection = read(|s| s.selection());
                view! {
                    <h3 class="tree-heading">"Cables"</h3>
                    <ul class="tree">
                        {rows
                            .into_iter()
                            .map(|row| {
                                let selected = matches!(row.target, RenameTarget::Object(id) if Some(id) == selection);
                                view! { <TreeRow row=row selected=selected collapsed=collapsed /> }
                            })
                            .collect_view()}
                    </ul>
                }
            })
        }}
    }
}

#[component]
fn TreeRow(row: Row, selected: bool, collapsed: RwSignal<Vec<ObjectId>>) -> impl IntoView {
    let sig = signals();
    let target = row.target;
    let editing = move || sig.renaming.get() == Some(target);
    let label = row.label.clone();
    let object = match target {
        RenameTarget::Object(id) => Some(id),
        RenameTarget::Document => None,
    };
    let rack_index = row.rack_index;
    let rack_id = match object {
        Some(ObjectId::Rack(r)) => Some(r),
        _ => None,
    };

    let on_click = move |_| update(|s| s.select(object));
    let on_dblclick = move |_| {
        if let Some(id) = object {
            update(|s| s.frame(id, now_s()));
        }
    };
    let on_context = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        match object {
            Some(id) => sig.menu.set(Some(Menu {
                x: ev.client_x() as f64,
                y: ev.client_y() as f64,
                target: id,
                origin: MenuOrigin::Tree,
            })),
            // The Document node only offers Rename: start it directly.
            None => sig.renaming.set(Some(RenameTarget::Document)),
        }
    };

    let dom_id = object.map(row_id);
    let expander = row.expander.map(|open| {
        let toggle = move |ev: leptos::ev::MouseEvent| {
            ev.stop_propagation();
            if let Some(id) = object {
                collapsed.update(|c| {
                    if open {
                        c.push(id);
                    } else {
                        c.retain(|x| *x != id);
                    }
                });
            }
        };
        view! {
            <button class="expander" aria-label=if open { "Collapse" } else { "Expand" } on:click=toggle>
                {if open { "▾" } else { "▸" }}
            </button>
        }
    });
    view! {
        <li
            id=dom_id
            class="tree-row"
            class:selected=selected
            style:padding-left=format!("{}px", 8 + row.depth * 16)
            draggable=if rack_id.is_some() { "true" } else { "false" }
            on:click=on_click
            on:dblclick=on_dblclick
            on:contextmenu=on_context
            on:dragstart=move |ev: leptos::ev::DragEvent| {
                if let Some(r) = rack_id {
                    if let Some(dt) = ev.data_transfer() {
                        let _ = dt.set_data("text/plain", "rack");
                    }
                    sig.tree_drag.set(Some(r));
                }
            }
            on:dragover=move |ev: leptos::ev::DragEvent| {
                if rack_index.is_some() && sig.tree_drag.get_untracked().is_some() {
                    ev.prevent_default();
                }
            }
            on:drop=move |ev: leptos::ev::DragEvent| {
                ev.prevent_default();
                if let (Some(dragged), Some(index)) = (sig.tree_drag.get_untracked(), rack_index) {
                    update(|s| s.move_rack(dragged, index));
                }
                sig.tree_drag.set(None);
            }
            on:dragend=move |_| sig.tree_drag.set(None)
        >
            {move || {
                if editing() {
                    view! { <InlineRename target=target initial=label.clone() /> }.into_any()
                } else {
                    view! {
                        <span class="tree-label">{expander.clone()}{label.clone()}</span>
                        <span class="tree-detail">{row.detail.clone()}</span>
                    }
                        .into_any()
                }
            }}
        </li>
    }
}

/// The inline name editor: Enter or blur commits, Esc cancels.
#[component]
fn InlineRename(target: RenameTarget, initial: String) -> impl IntoView {
    let sig = signals();
    let text = RwSignal::new(initial);
    let error = RwSignal::new(None::<String>);
    let input_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = input_ref.get() {
            let _ = input.focus();
            input.select();
        }
    });
    let commit = move || {
        if sig.renaming.get_untracked() != Some(target) {
            return;
        }
        let value = text.get_untracked();
        let result = update(|s| match target {
            RenameTarget::Document => s.rename_document(&value),
            RenameTarget::Object(id) => s.rename(id, &value),
        });
        match result {
            Ok(()) => sig.renaming.set(None),
            Err(e) => error.set(Some(e)),
        }
    };
    view! {
        <input
            node_ref=input_ref
            class="inline-rename"
            prop:value=move || text.get()
            on:input=move |ev| {
                text.set(event_target_value(&ev));
                error.set(None);
            }
            on:keydown=move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
                "Enter" => commit(),
                "Escape" => sig.renaming.set(None),
                _ => {}
            }
            on:blur=move |_| commit()
            on:click=|ev: leptos::ev::MouseEvent| ev.stop_propagation()
        />
        {move || error.get().map(|e| view! { <div class="field-error">{e}</div> })}
    }
}
