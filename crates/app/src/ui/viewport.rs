//! The 3D view: canvas, pointer input, trash zone, context menu, save banner (spec §4.3).

use glam::Vec2;
use leptos::html;
use leptos::prelude::*;
use rackwright_render::gpu::Renderer;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::interaction::Button;
use crate::session::DeleteOutcome;
use crate::ui::core::{
    Dialog, Menu, MenuOrigin, RenameTarget, core, now_s, read, request_frame, signals, update,
};

/// Canvas-relative position of a pointer event (CSS px) and whether it is over the trash zone.
pub fn pointer_info(client_x: i32, client_y: i32) -> (Vec2, bool) {
    let core = core();
    let (x, y) = (client_x as f64, client_y as f64);
    let pos = core.canvas.borrow().as_ref().map_or(Vec2::ZERO, |c| {
        let r = c.get_bounding_client_rect();
        Vec2::new((x - r.left()) as f32, (y - r.top()) as f32)
    });
    let over_trash = core.trash.borrow().as_ref().is_some_and(|t| {
        let r = t.get_bounding_client_rect();
        x >= r.left() && x <= r.right() && y >= r.top() && y <= r.bottom()
    });
    (pos, over_trash)
}

/// Forwards a pointer move from whichever element captured the pointer.
pub fn forward_move(ev: &web_sys::PointerEvent) {
    // A gesture whose button is no longer held lost its release (e.g. alt-tab mid-drag).
    if ev.buttons() == 0 && read(|s| s.is_busy()) {
        forward_cancel();
        return;
    }
    let (pos, over_trash) = pointer_info(ev.client_x(), ev.client_y());
    let sig = signals();
    if sig.over_trash.get_untracked() != over_trash {
        sig.over_trash.set(over_trash);
    }
    update(|s| s.pointer_move(pos, over_trash, now_s()));
}

pub fn forward_up(ev: &web_sys::PointerEvent) {
    let (pos, over_trash) = pointer_info(ev.client_x(), ev.client_y());
    signals().over_trash.set(false);
    update(|s| s.pointer_up(pos, over_trash, now_s()));
}

/// The browser cancelled the pointer or released its capture (after a normal release this
/// finds the session already idle and does nothing).
pub fn forward_cancel() {
    signals().over_trash.set(false);
    update(|s| s.pointer_cancel(now_s()));
}

fn button(ev: &web_sys::PointerEvent) -> Option<Button> {
    match ev.button() {
        0 => Some(Button::Left),
        1 => Some(Button::Middle),
        2 => Some(Button::Right),
        _ => None,
    }
}

/// Runs a context-menu action for `menu`.
pub fn menu_rename(menu: Menu) {
    let sig = signals();
    sig.menu.set(None);
    update(|s| s.select(Some(menu.target)));
    match menu.origin {
        MenuOrigin::Tree => sig.renaming.set(Some(RenameTarget::Object(menu.target))),
        MenuOrigin::View => sig.focus_name.set(Some(menu.target)),
    }
}

pub fn menu_delete(menu: Menu) {
    let sig = signals();
    sig.menu.set(None);
    if update(|s| s.request_delete(menu.target)) == DeleteOutcome::NeedsConfirmation
        && let rackwright_core::edit::ObjectId::Rack(rack) = menu.target
    {
        let name = read(|s| {
            s.document()
                .rack(rack)
                .map(|r| r.name.to_string())
                .unwrap_or_default()
        });
        sig.dialog
            .set(Some(Dialog::ConfirmDeleteRack { rack, name }));
    }
}

#[component]
pub fn Viewport() -> impl IntoView {
    let sig = signals();
    let canvas_ref = NodeRef::<html::Canvas>::new();
    let trash_ref = NodeRef::<html::Div>::new();

    Effect::new(move |_| {
        let (Some(canvas), Some(trash)) = (canvas_ref.get(), trash_ref.get()) else {
            return;
        };
        let core = core();
        if core.canvas.borrow().is_some() {
            return;
        }
        *core.canvas.borrow_mut() = Some(canvas.clone());
        *core.trash.borrow_mut() = Some(trash.unchecked_into());
        observe_size(&canvas);
        leptos::task::spawn_local(async move {
            match Renderer::for_canvas(canvas).await {
                Ok(renderer) => {
                    *core.renderer.borrow_mut() = Some(renderer);
                    resize_to_canvas();
                    request_frame();
                }
                Err(e) => core.sig.dialog.set(Some(Dialog::Error {
                    message: format!("The 3D view could not start: {e}"),
                })),
            }
        });
    });

    let on_down = move |ev: leptos::ev::PointerEvent| {
        let Some(b) = button(&ev) else { return };
        signals().menu.set(None);
        if let Some(target) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        let (pos, _) = pointer_info(ev.client_x(), ev.client_y());
        update(|s| s.pointer_down_with_shift(pos, b, ev.shift_key()));
    };
    let on_context = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        let (pos, _) = pointer_info(ev.client_x(), ev.client_y());
        if let Some(target) = read(|s| s.context_target(pos)) {
            sig.menu.set(Some(Menu {
                x: ev.client_x() as f64,
                y: ev.client_y() as f64,
                target,
                origin: MenuOrigin::View,
            }));
        }
    };
    let on_dblclick = move |ev: leptos::ev::MouseEvent| {
        let (pos, _) = pointer_info(ev.client_x(), ev.client_y());
        update(|s| s.double_click(pos, now_s()));
    };
    let on_wheel = move |ev: leptos::ev::WheelEvent| {
        ev.prevent_default();
        update(|s| s.wheel(ev.delta_y() as f32));
    };

    view! {
        <div class="viewport">
            <canvas
                node_ref=canvas_ref
                on:pointerdown=on_down
                on:pointermove=move |ev: leptos::ev::PointerEvent| forward_move(&ev)
                on:pointerup=move |ev: leptos::ev::PointerEvent| forward_up(&ev)
                on:pointercancel=move |_| forward_cancel()
                on:lostpointercapture=move |_| forward_cancel()
                on:dblclick=on_dblclick
                on:contextmenu=on_context
                on:wheel=on_wheel
            ></canvas>
            <div
                node_ref=trash_ref
                class="trash"
                class:visible=move || sig.dragging.get()
                class:hot=move || sig.over_trash.get()
            >
                "Drop here to delete"
            </div>
            <ControlsHelp />
            {move || {
                sig.rev.track();
                read(|s| s.banner().map(str::to_string))
                    .map(|text| view! { <div class="banner">{text}</div> })
            }}
            {move || sig.menu.get().map(|menu| view! { <ContextMenu menu=menu /> })}
        </div>
    }
}

/// The mouse controls, shown while the pointer is over the "?" in the corner of the view.
#[component]
pub fn ControlsHelp() -> impl IntoView {
    const LINES: [(&str, &str); 7] = [
        ("Left-drag", "rotate"),
        ("Middle-drag", "pan"),
        ("Wheel", "zoom"),
        ("Left-drag a device/port", "move it"),
        ("Shift-drag a port", "connect or re-plug a cable"),
        ("Double-click", "focus"),
        ("Right-click", "rename/delete"),
    ];
    view! {
        <div class="controls-help">
            <div class="controls-help-panel">
                {LINES
                    .iter()
                    .map(|(input, action)| view! { <div><b>{*input}":"</b>" "{*action}</div> })
                    .collect_view()}
            </div>
            <div class="controls-help-toggle">"?"</div>
        </div>
    }
}

#[component]
pub fn ContextMenu(menu: Menu) -> impl IntoView {
    view! {
        <div
            class="context-menu"
            style:left=format!("{}px", menu.x)
            style:top=format!("{}px", menu.y)
            on:pointerdown=|ev: leptos::ev::PointerEvent| ev.stop_propagation()
        >
            <button on:click=move |_| menu_rename(menu)>"Rename"</button>
            <button on:click=move |_| menu_delete(menu)>"Delete"</button>
        </div>
    }
}

thread_local! {
    /// The canvas's exact size in device pixels, when the browser reports it.
    static DEVICE_PIXELS: std::cell::Cell<Option<(u32, u32)>> = const { std::cell::Cell::new(None) };
}

/// Whether `ResizeObserver` reports sizes in device pixels (not in Safari).
fn device_pixel_box_supported() -> bool {
    let key = wasm_bindgen::JsValue::from_str("devicePixelContentBoxSize");
    js_sys::Reflect::get(&js_sys::global(), &"ResizeObserverEntry".into())
        .and_then(|class| js_sys::Reflect::get(&class, &"prototype".into()))
        .and_then(|prototype| js_sys::Reflect::has(prototype.unchecked_ref(), &key))
        .unwrap_or(false)
}

/// Keeps the canvas's pixel size, the renderer and the session viewport equal to its CSS size.
/// Where the browser reports the canvas's size in device pixels, that size is used as is, so
/// fractional display scaling (125 %, 150 %) maps canvas pixels one-to-one onto the screen.
fn observe_size(canvas: &web_sys::HtmlCanvasElement) {
    let device_pixels = device_pixel_box_supported();
    let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
        let entry = entries.iter().last();
        if let Some(entry) = entry.filter(|_| device_pixels) {
            let entry: web_sys::ResizeObserverEntry = entry.unchecked_into();
            let size: web_sys::ResizeObserverSize = entry
                .device_pixel_content_box_size()
                .get(0)
                .unchecked_into();
            let size = (
                size.inline_size().round() as u32,
                size.block_size().round() as u32,
            );
            DEVICE_PIXELS.with(|cell| cell.set(Some(size)));
        }
        resize_to_canvas();
    });
    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        if device_pixels {
            // Also fires when only the device pixel ratio changes (moving to another screen).
            let options = web_sys::ResizeObserverOptions::new();
            options.set_box(web_sys::ResizeObserverBoxOptions::DevicePixelContentBox);
            observer.observe_with_options(canvas, &options);
        } else {
            observer.observe(canvas);
        }
    }
    callback.forget();
}

fn resize_to_canvas() {
    let core = core();
    let Some(canvas) = core.canvas.borrow().clone() else {
        return;
    };
    let rect = canvas.get_bounding_client_rect();
    let (width, height) = DEVICE_PIXELS.with(|cell| cell.get()).unwrap_or_else(|| {
        let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
        (
            (rect.width() * dpr).round() as u32,
            (rect.height() * dpr).round() as u32,
        )
    });
    if let Some(renderer) = core.renderer.borrow_mut().as_mut() {
        renderer.resize(width, height);
        let (w, h) = renderer.size();
        canvas.set_width(w);
        canvas.set_height(h);
    }
    let css = Vec2::new(rect.width() as f32, rect.height() as f32);
    update(|s| s.set_viewport(css));
}
