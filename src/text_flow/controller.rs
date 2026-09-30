use super::{chips::*, geometry::*, FlowLayout, PositionedLine};
use crate::pretext::{browser::*, *};
use leptos::prelude::*;
use std::collections::BTreeMap;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{Event, EventTarget, HtmlElement, ResizeObserver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObstacleKind {
    Photo,
    FloatingCertificate,
}
#[derive(Clone, Copy)]
pub struct FlowContext {
    state: StoredValue<Controller, LocalStorage>,
}

struct Listener {
    target: EventTarget,
    name: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.name, self.callback.as_ref().unchecked_ref());
    }
}
struct TextBlock {
    element: HtmlElement,
    source: String,
    options: PrepareOptions,
    anywhere: bool,
    output: RwSignal<Option<FlowLayout>>,
    prepared: Option<PreparedTextWithSegments>,
    font: String,
    line_height: f64,
    min_word: f64,
    version: u64,
    last_key: String,
}
struct ChipBlock {
    element: HtmlElement,
    exclude_certificates: bool,
    last_key: String,
    children: Vec<HtmlElement>,
    sizes: Vec<ChipSize>,
    measure_dirty: bool,
}
struct Obstacle {
    element: HtmlElement,
    kind: ObstacleKind,
}
struct Controller {
    engine: Option<TextEngine<BrowserBackend>>,
    texts: BTreeMap<u64, TextBlock>,
    chips: BTreeMap<u64, ChipBlock>,
    obstacles: BTreeMap<u64, Obstacle>,
    next_id: u64,
    frame: Option<i32>,
    frame_callback: Option<Closure<dyn FnMut(f64)>>,
    observer: Option<ResizeObserver>,
    observer_callback: Option<Closure<dyn FnMut(js_sys::Array, ResizeObserver)>>,
    listeners: Vec<Listener>,
    font_generation: u64,
    active: bool,
    settle: bool,
}
impl FlowContext {
    pub fn provide() -> Self {
        let engine = BrowserBackend::new().map(|b| TextEngine::new(b, browser_profile()));
        if let Err(e) = &engine {
            leptos::logging::error!("text flow unavailable: {e}");
        }
        let state = StoredValue::new_local(Controller {
            engine: engine.ok(),
            texts: BTreeMap::new(),
            chips: BTreeMap::new(),
            obstacles: BTreeMap::new(),
            next_id: 1,
            frame: None,
            frame_callback: None,
            observer: None,
            observer_callback: None,
            listeners: Vec::new(),
            font_generation: 0,
            active: true,
            settle: false,
        });
        let context = Self { state };
        provide_context(context);
        state.update_value(|s| {
            s.frame_callback = Some(Closure::wrap(
                Box::new(move |_| context.tick()) as Box<dyn FnMut(f64)>
            ));
            let cb =
                Closure::wrap(
                    Box::new(move |_: js_sys::Array, _: ResizeObserver| context.resized())
                        as Box<dyn FnMut(js_sys::Array, ResizeObserver)>,
                );
            if let Ok(observer) = ResizeObserver::new(cb.as_ref().unchecked_ref()) {
                s.observer = Some(observer);
                s.observer_callback = Some(cb);
            }
        });
        if let Some(window) = web_sys::window() {
            context.listen(window.clone().unchecked_into(), "resize", move |_| {
                context.schedule()
            });
            if let Some(document) = window.document() {
                context.listen(document.clone().unchecked_into(), "photomove", move |_| {
                    context.schedule()
                });
                let fonts = document.fonts();
                context.listen(fonts.clone().unchecked_into(), "loadingdone", move |_| {
                    context.fonts_changed()
                });
                context.listen(fonts.clone().unchecked_into(), "loadingerror", move |_| {
                    context.fonts_changed()
                });
                if let Ok(ready) = fonts.ready() {
                    leptos::task::spawn_local(async move {
                        let _ = wasm_bindgen_futures::JsFuture::from(ready).await;
                        context.fonts_changed();
                    });
                }
            }
        }
        on_cleanup(move || {
            context.state.try_update_value(|s| s.dispose());
        });
        context
    }
    fn listen(
        self,
        target: EventTarget,
        name: &'static str,
        callback: impl FnMut(Event) + 'static,
    ) {
        let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
        if target
            .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
            .is_ok()
        {
            self.state.update_value(|s| {
                s.listeners.push(Listener {
                    target,
                    name,
                    callback,
                })
            });
        }
    }
    pub fn schedule(self) {
        self.state.try_update_value(|s| {
            if !s.active || s.frame.is_some() || s.engine.is_none() {
                return;
            }
            if let (Some(window), Some(callback)) = (web_sys::window(), s.frame_callback.as_ref()) {
                s.frame = window
                    .request_animation_frame(callback.as_ref().unchecked_ref())
                    .ok();
            }
        });
    }
    pub fn register_text(
        self,
        element: HtmlElement,
        source: String,
        options: PrepareOptions,
        anywhere: bool,
        output: RwSignal<Option<FlowLayout>>,
    ) -> u64 {
        let id = self.state.with_value(|s| s.next_id);
        self.state.update_value(|s| {
            s.next_id += 1;
            if let Some(o) = &s.observer {
                o.observe(&element);
            }
            s.texts.insert(
                id,
                TextBlock {
                    element,
                    source,
                    options,
                    anywhere,
                    output,
                    prepared: None,
                    font: String::new(),
                    line_height: 0.0,
                    min_word: 0.0,
                    version: 0,
                    last_key: String::new(),
                },
            );
        });
        self.schedule();
        id
    }
    pub fn update_text(self, id: u64, source: String, options: PrepareOptions) {
        self.state.try_update_value(|s| {
            if let Some(b) = s.texts.get_mut(&id) {
                if b.source != source || b.options != options {
                    b.source = source;
                    b.options = options;
                    b.prepared = None;
                    b.version += 1;
                    b.last_key.clear();
                }
            }
        });
        self.schedule();
    }
    pub fn register_chips(self, element: HtmlElement, exclude_certificates: bool) -> u64 {
        let id = self.state.with_value(|s| s.next_id);
        self.state.update_value(|s| {
            s.next_id += 1;
            if let Some(o) = &s.observer {
                o.observe(&element);
            }
            s.chips.insert(
                id,
                ChipBlock {
                    element,
                    exclude_certificates,
                    last_key: String::new(),
                    children: Vec::new(),
                    sizes: Vec::new(),
                    measure_dirty: true,
                },
            );
        });
        self.schedule();
        id
    }
    pub fn register_obstacle(self, element: HtmlElement, kind: ObstacleKind) -> u64 {
        let id = self.state.with_value(|s| s.next_id);
        self.state.update_value(|s| {
            s.next_id += 1;
            if let Some(o) = &s.observer {
                o.observe(&element);
            }
            s.obstacles.insert(id, Obstacle { element, kind });
        });
        self.schedule();
        id
    }
    pub fn unregister(self, id: u64) {
        self.state.try_update_value(|s| {
            let element = s
                .texts
                .remove(&id)
                .map(|b| b.element)
                .or_else(|| {
                    s.chips.remove(&id).map(|b| {
                        if let Some(o) = &s.observer {
                            for child in &b.children {
                                o.unobserve(child);
                            }
                        }
                        b.element
                    })
                })
                .or_else(|| s.obstacles.remove(&id).map(|b| b.element));
            if let (Some(o), Some(e)) = (&s.observer, element) {
                o.unobserve(&e);
            }
        });
        self.schedule();
    }
    fn resized(self) {
        self.state.try_update_value(|s| {
            for b in s.chips.values_mut() {
                b.measure_dirty = true;
            }
        });
        self.schedule();
    }
    fn fonts_changed(self) {
        self.state.try_update_value(|s| {
            if !s.active {
                return;
            }
            s.font_generation += 1;
            if let Some(e) = &mut s.engine {
                e.clear_cache();
            }
            for b in s.texts.values_mut() {
                b.prepared = None;
                b.last_key.clear();
            }
            for b in s.chips.values_mut() {
                b.last_key.clear();
                b.measure_dirty = true;
            }
        });
        self.schedule();
    }
    fn tick(self) {
        let mut changed = false;
        let mut resettle = false;
        self.state.try_update_value(|s| {
            s.frame = None;
            if !s.active {
                return;
            }
            let Some(engine) = &mut s.engine else {
                return;
            };
            let Some(window) = web_sys::window() else {
                return;
            };
            let mut obstacles = Vec::new();
            for obstacle in s.obstacles.values() {
                if !obstacle.element.is_connected()
                    || (obstacle.kind == ObstacleKind::FloatingCertificate
                        && obstacle.element.get_attribute("data-floating").as_deref() != Some("1"))
                {
                    continue;
                }
                let r = obstacle.element.get_bounding_client_rect();
                if r.width() > 0.0 && r.height() > 0.0 {
                    obstacles.push((
                        Circle {
                            cx: r.x() + r.width() / 2.0,
                            cy: r.y() + r.height() / 2.0,
                            radius: r.width().min(r.height()) / 2.0,
                        },
                        obstacle.kind,
                    ));
                }
            }
            // Measure every host first. Reactively applying heights can move later hosts;
            // a single follow-up frame measures their settled positions.
            let text_boxes: Vec<_> = s
                .texts
                .iter()
                .map(|(id, b)| {
                    (
                        *id,
                        b.element.get_bounding_client_rect(),
                        window.get_computed_style(&b.element).ok().flatten(),
                    )
                })
                .collect();
            let chip_boxes: Vec<_> = s
                .chips
                .iter()
                .map(|(id, b)| (*id, b.element.get_bounding_client_rect()))
                .collect();
            for (id, rect, css) in text_boxes {
                let b = s.texts.get_mut(&id).unwrap();
                if rect.width() <= 0.0 || !b.element.is_connected() {
                    continue;
                }
                let Some(css) = css else {
                    continue;
                };
                let property = |name: &str| css.get_property_value(name).unwrap_or_default();
                let mut font = property("font");
                if font.trim().is_empty() {
                    font = format!(
                        "{} {} {} {}",
                        property("font-style"),
                        property("font-weight"),
                        property("font-size"),
                        property("font-family")
                    );
                }
                let height = px(&property("line-height"))
                    .unwrap_or_else(|| px(&property("font-size")).unwrap_or(16.0) * 1.5)
                    .max(1.0);
                let spacing = px(&property("letter-spacing")).unwrap_or(0.0);
                let options = PrepareOptions {
                    letter_spacing: spacing,
                    ..b.options
                };
                if b.prepared.is_none() || b.font != font || b.options.letter_spacing != spacing {
                    b.options = options;
                    match engine.prepare_with_segments(&b.source, &font, options) {
                        Ok(prepared) => {
                            b.prepared = Some(prepared);
                            b.font = font.clone();
                            b.version += 1;
                            b.last_key.clear();
                            let words = b.source.split_whitespace().collect::<Vec<_>>().join("\n");
                            b.min_word = engine
                                .prepare(
                                    &words,
                                    &font,
                                    PrepareOptions {
                                        white_space: WhiteSpace::PreWrap,
                                        ..options
                                    },
                                )
                                .and_then(|p| measure_natural_width(&p))
                                .unwrap_or(0.0);
                        }
                        Err(e) => {
                            leptos::logging::error!("text preparation failed: {e}");
                            b.output.set(None);
                            continue;
                        }
                    }
                }
                b.line_height = height;
                let circles: Vec<_> = obstacles
                    .iter()
                    .map(|(c, _)| Circle {
                        cx: c.cx - rect.x(),
                        cy: c.cy - rect.y(),
                        radius: c.radius,
                    })
                    .collect();
                let key = format!("{}|{}|{}|{:?}", rect.width(), height, b.version, circles);
                if key == b.last_key {
                    continue;
                }
                let prepared = b.prepared.as_ref().unwrap();
                let mut cursor = LayoutCursor::default();
                let mut y = 0.0;
                let mut lines = Vec::new();
                let mut failed = false;
                let min_gap = if b.anywhere {
                    MIN_LINE_WIDTH
                } else {
                    MIN_LINE_WIDTH.max(b.min_word.ceil())
                };
                while cursor.segment_index < prepared.segments.len() {
                    let intervals = free_intervals(rect.width(), &circles, y, height, min_gap);
                    for interval in intervals {
                        match layout_next_line_range(
                            &prepared.geometry,
                            cursor,
                            interval.width.max(1.0),
                        ) {
                            Ok(Some(range)) => {
                                if range.end <= cursor {
                                    failed = true;
                                    break;
                                }
                                let hard = range.end.grapheme_index == 0
                                    && range.end.segment_index > 0
                                    && prepared.kinds[range.end.segment_index - 1]
                                        == SegmentKind::HardBreak;
                                let mut line = materialize_line_range(prepared, &range).unwrap();
                                if hard {
                                    line.text.push('\n');
                                }
                                lines.push(PositionedLine {
                                    text: line.text,
                                    x: interval.x,
                                    y,
                                });
                                cursor = range.end;
                                if hard {
                                    break;
                                }
                            }
                            Ok(None) => {
                                cursor.segment_index = prepared.segments.len();
                                break;
                            }
                            Err(e) => {
                                leptos::logging::error!("text layout failed: {e}");
                                failed = true;
                                break;
                            }
                        }
                    }
                    if failed {
                        break;
                    }
                    y += height;
                }
                if failed {
                    b.output.set(None);
                    continue;
                }
                let total = lines.last().map_or(height, |l| l.y + height);
                let layout = FlowLayout {
                    lines,
                    height: total,
                    line_height: height,
                };
                let different = b.output.with_untracked(|old| old.as_ref() != Some(&layout));
                if different {
                    changed |= (rect.height() - total).abs() > 0.5;
                    b.output.set(Some(layout));
                }
                b.last_key = key;
            }
            for (id, rect) in chip_boxes {
                let b = s.chips.get_mut(&id).unwrap();
                if rect.width() <= 0.0 || !b.element.is_connected() {
                    continue;
                }
                if b.measure_dirty || b.element.children().length() as usize != b.children.len() {
                    if let Some(o) = &s.observer {
                        for child in &b.children {
                            o.unobserve(child);
                        }
                    }
                    let children = b.element.children();
                    b.children = (0..children.length())
                        .filter_map(|i| children.item(i).and_then(|e| e.dyn_into().ok()))
                        .collect();
                    for chip in &b.children {
                        let style = chip.style();
                        let _ = style.set_property("max-width", "100%");
                        let _ = style.set_property("min-width", "0");
                        let _ = style.set_property("width", "max-content");
                        let _ = style.set_property("overflow-wrap", "anywhere");
                        let _ = style.set_property("position", "absolute");
                    }
                    b.sizes = b
                        .children
                        .iter()
                        .map(|chip| {
                            if let Some(o) = &s.observer {
                                o.observe(chip);
                            }
                            let r = chip.get_bounding_client_rect();
                            ChipSize {
                                width: r.width(),
                                height: r.height(),
                            }
                        })
                        .collect();
                    b.measure_dirty = false;
                }
                let css = window.get_computed_style(&b.element).ok().flatten();
                let gap = css
                    .and_then(|c| px(&c.get_property_value("column-gap").unwrap_or_default()))
                    .unwrap_or(0.0);
                let circles: Vec<_> = obstacles
                    .iter()
                    .filter(|(_, kind)| {
                        !b.exclude_certificates || *kind != ObstacleKind::FloatingCertificate
                    })
                    .map(|(c, _)| Circle {
                        cx: c.cx - rect.x(),
                        cy: c.cy - rect.y(),
                        radius: c.radius,
                    })
                    .collect();
                let key = format!(
                    "{}|{}|{}|{:?}|{:?}",
                    rect.width(),
                    gap,
                    s.font_generation,
                    b.sizes,
                    circles
                );
                if key == b.last_key {
                    continue;
                }
                let (positions, height) = pack_chips(rect.width(), &b.sizes, &circles, gap);
                let _ = b.element.style().set_property("position", "relative");
                let _ = b
                    .element
                    .style()
                    .set_property("height", &format!("{height}px"));
                for (chip, position) in b.children.iter().zip(positions) {
                    let _ = chip
                        .style()
                        .set_property("left", &format!("{}px", position.x));
                    let _ = chip
                        .style()
                        .set_property("top", &format!("{}px", position.y));
                }
                changed |= (rect.height() - height).abs() > 0.5;
                b.last_key = key;
            }
            resettle = changed && !s.settle;
            s.settle = resettle;
        });
        if resettle {
            self.schedule();
        }
    }
}
impl Controller {
    fn dispose(&mut self) {
        self.active = false;
        if let (Some(window), Some(frame)) = (web_sys::window(), self.frame.take()) {
            let _ = window.cancel_animation_frame(frame);
        }
        if let Some(observer) = self.observer.take() {
            observer.disconnect();
        }
        self.listeners.clear();
        self.texts.clear();
        self.chips.clear();
        self.obstacles.clear();
    }
}
fn px(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix("px")
        .unwrap_or(value)
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}
