use leptos::portal::Portal;
use leptos::prelude::*;

#[derive(Clone, Copy)]
struct Position {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// The docked image keeps its slot; Leptos owns a separate floating portal.
#[component]
pub fn CertificateIcon(#[prop(into)] src: String) -> impl IntoView {
    let docked = NodeRef::<leptos::html::Img>::new();
    let overlay = NodeRef::<leptos::html::Img>::new();
    let position = RwSignal::new(None::<Position>);
    let suppress_until = RwSignal::new(0.0);
    #[cfg(feature = "hydrate")]
    let start = {
        use crate::text_flow::controller::{FlowContext, ObstacleKind};
        use wasm_bindgen::{closure::Closure, JsCast};
        #[derive(Clone, Copy)]
        struct Drag {
            x: f64,
            y: f64,
            origin: Position,
            moved: bool,
            pointer: i32,
        }
        let drag = StoredValue::new(None::<Drag>);
        let context = use_context::<FlowContext>();
        if let Some(context) = context {
            let id = StoredValue::new(None::<u64>);
            overlay.on_load(move |el| {
                id.set_value(Some(context.register_obstacle(
                    el.unchecked_into(),
                    ObstacleKind::FloatingCertificate,
                )))
            });
            on_cleanup(move || {
                if let Some(id) = id.get_value() {
                    context.unregister(id);
                }
            });
        }
        let window = web_sys::window().expect("browser window");
        let move_callback = Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
            let Some(mut current) = drag.get_value() else {
                return;
            };
            if event.pointer_id() != current.pointer {
                return;
            }
            let dx = event.client_x() as f64 - current.x;
            let dy = event.client_y() as f64 - current.y;
            if !current.moved && dx.hypot(dy) < 4.0 {
                return;
            }
            current.moved = true;
            drag.set_value(Some(current));
            position.set(Some(Position {
                x: current.origin.x + dx,
                y: current.origin.y + dy,
                ..current.origin
            }));
            if let Some(context) = context {
                context.schedule();
            }
            event.prevent_default();
        }) as Box<dyn FnMut(web_sys::PointerEvent)>);
        let end_callback = Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
            let Some(current) = drag.get_value() else {
                return;
            };
            if event.pointer_id() != current.pointer {
                return;
            }
            drag.set_value(None);
            if !current.moved {
                return;
            }
            suppress_until.set(js_sys::Date::now() + 400.0);
            if let (Some(p), Some(el)) = (position.get_untracked(), docked.get_untracked()) {
                if let Ok(Some(card)) = el.closest(".cert-card") {
                    let bounds = card.get_bounding_client_rect();
                    let window = web_sys::window().unwrap();
                    let x = p.x + p.width / 2.0 - window.scroll_x().unwrap_or(0.0);
                    let y = p.y + p.height / 2.0 - window.scroll_y().unwrap_or(0.0);
                    if x >= bounds.left()
                        && x <= bounds.right()
                        && y >= bounds.top()
                        && y <= bounds.bottom()
                    {
                        position.set(None);
                    }
                }
            }
            if let Some(context) = context {
                context.schedule();
            }
        }) as Box<dyn FnMut(web_sys::PointerEvent)>);
        window
            .add_event_listener_with_callback("pointermove", move_callback.as_ref().unchecked_ref())
            .unwrap();
        window
            .add_event_listener_with_callback("pointerup", end_callback.as_ref().unchecked_ref())
            .unwrap();
        window
            .add_event_listener_with_callback(
                "pointercancel",
                end_callback.as_ref().unchecked_ref(),
            )
            .unwrap();
        let listeners = StoredValue::new_local(Some((window, move_callback, end_callback)));
        on_cleanup(move || {
            listeners.try_update_value(|listeners| {
                if let Some((window, move_callback, end_callback)) = listeners.take() {
                    let _ = window.remove_event_listener_with_callback(
                        "pointermove",
                        move_callback.as_ref().unchecked_ref(),
                    );
                    let _ = window.remove_event_listener_with_callback(
                        "pointerup",
                        end_callback.as_ref().unchecked_ref(),
                    );
                    let _ = window.remove_event_listener_with_callback(
                        "pointercancel",
                        end_callback.as_ref().unchecked_ref(),
                    );
                }
            });
        });
        move |event: leptos::ev::PointerEvent| {
            if event.button() != 0 {
                return;
            }
            let Some(el) = docked.get_untracked() else {
                return;
            };
            let bounds = el.get_bounding_client_rect();
            let window = web_sys::window().unwrap();
            let origin = position.get_untracked().unwrap_or(Position {
                x: bounds.left() + window.scroll_x().unwrap_or(0.0),
                y: bounds.top() + window.scroll_y().unwrap_or(0.0),
                width: bounds.width(),
                height: bounds.height(),
            });
            drag.set_value(Some(Drag {
                x: event.client_x() as f64,
                y: event.client_y() as f64,
                origin,
                moved: false,
                pointer: event.pointer_id(),
            }));
        }
    };
    #[cfg(not(feature = "hydrate"))]
    let start = |_: leptos::ev::PointerEvent| {};
    let click = move |event: leptos::ev::MouseEvent| {
        #[cfg(feature = "hydrate")]
        if js_sys::Date::now() < suppress_until.get_untracked() {
            event.prevent_default();
            event.stop_propagation();
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = (event, suppress_until);
    };
    let floating_src = src.clone();
    view! {
        <img node_ref=docked class="cert-icon" src=src alt="" draggable="false"
            style:visibility=move || if position.get().is_some() { "hidden" } else { "visible" }
            on:pointerdown=start on:click=click/>
        <Portal>
            <img node_ref=overlay class="cert-icon" src=floating_src.clone() alt="" draggable="false"
                data-floating=move || position.get().map(|_| "1")
                style=move || position.get().map(|p| format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:50",p.x,p.y,p.width,p.height)).unwrap_or_else(|| "display:none".into())
                on:pointerdown=start on:click=click/>
        </Portal>
    }
}
