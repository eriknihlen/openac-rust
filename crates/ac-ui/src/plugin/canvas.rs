//! The plugins' canvases on the overlay band (OpenAC
//! `PluginCanvasElement`): each a rectangle at its anchor plus offset,
//! over the world and under every window, showing its retained paint list
//! and, when it opted in and its plugin listens, taking the pointer inside
//! its rectangle: a press holds the pointer until the button comes up, the
//! moves and the release reach the plugin wherever they happen, and a press
//! that ends otherwise is reported cancelled.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ac_plugin_ui::{
    CanvasPlace, CanvasView, PaintCmd, PluginUiView, PointerEvent, PointerKind, UiInput,
    canvas_origin,
};

use crate::draw::{Color, DEFAULT_OUTLINE_COLOR, RenderContext};
use crate::event::{UiEvent, kind};
use crate::font::UiFont;
use crate::geometry::AnchorEdges;
use crate::overlay::OverlayHost;
use crate::root::UiRoot;
use crate::tree::{Element, ElementId, Widget};

use super::{Icon, PluginOutbox};

/// The overlay band layer the canvases sit on.
pub const LAYER_NAME: &str = "PluginCanvasLayer";

/// One canvas on the screen.
#[derive(Debug)]
pub struct CanvasWidget {
    /// The canvas as the session's copy has it.
    pub canvas: CanvasView,
    /// Its plugin's images by handle.
    pub images: HashMap<u32, Icon>,
    font: Option<Arc<UiFont>>,
    outbox: PluginOutbox,
    held: Option<u8>,
    releases_seen: u32,
    place: CanvasPlace,
}

fn color(c: ac_plugin_ui::Color8) -> Color {
    let c = c.to_rgba();
    Color::rgba(c.r, c.g, c.b, c.a)
}

impl CanvasWidget {
    fn deliver(&self, kind: PointerKind, x: f32, y: f32, button: u8, modifiers: u8, wheel: i32) {
        self.outbox.borrow_mut().push(UiInput::Pointer {
            canvas: self.canvas.id,
            event: PointerEvent {
                kind,
                x,
                y,
                button,
                modifiers,
                wheel_delta: wheel,
            },
            place: self.place,
        });
    }

    /// Whether a press is held.
    #[must_use]
    pub fn holding(&self) -> bool {
        self.held.is_some()
    }

    fn press(&mut self, ui: &mut UiRoot, me: ElementId, button: u8, e: &UiEvent) -> bool {
        if !self.canvas.takes_input() {
            return false;
        }
        if self.held.is_none() {
            self.held = Some(button);
            ui.set_capture(me);
        }
        self.deliver(
            PointerKind::Down,
            e.data1 as f32,
            e.data2 as f32,
            button,
            e.modifiers.0 & 7,
            0,
        );
        true
    }

    fn release(&mut self, ui: &mut UiRoot, me: ElementId, button: u8, e: &UiEvent) -> bool {
        if self.held != Some(button) {
            return false;
        }
        self.held = None;
        if ui.captured() == Some(me) {
            ui.release_capture();
        }
        self.deliver(
            PointerKind::Up,
            e.data1 as f32,
            e.data2 as f32,
            button,
            e.modifiers.0 & 7,
            0,
        );
        true
    }
}

impl Widget for CanvasWidget {
    fn hit_test(&self, element: &Element, x: f32, y: f32) -> bool {
        self.canvas.takes_input() && element.contains_local(x, y)
    }

    fn handles_click(&self, _element: &Element) -> bool {
        self.canvas.takes_input()
    }

    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if e.target != Some(me) {
            return false;
        }
        match e.kind {
            kind::MOUSE_DOWN => self.press(ui, me, 1, e),
            kind::RIGHT_DOWN => self.press(ui, me, 2, e),
            kind::MIDDLE_DOWN => self.press(ui, me, 3, e),
            kind::MOUSE_UP => self.release(ui, me, 1, e),
            kind::RIGHT_UP => self.release(ui, me, 2, e),
            kind::MIDDLE_UP => self.release(ui, me, 3, e),
            kind::MOUSE_MOVE => match self.held {
                Some(button) => {
                    self.deliver(
                        PointerKind::Move,
                        e.data1 as f32,
                        e.data2 as f32,
                        button,
                        e.modifiers.0 & 7,
                        0,
                    );
                    true
                }
                None => false,
            },
            kind::SCROLL if self.canvas.takes_input() => {
                let (mx, my) = ui.mouse();
                let (sx, sy) = ui.screen_position(me);
                self.deliver(
                    PointerKind::Wheel,
                    mx as f32 - sx,
                    my as f32 - sy,
                    0,
                    e.modifiers.0 & 7,
                    e.data0.signum(),
                );
                true
            }
            kind::CAPTURE_CHANGED => match self.held.take() {
                Some(button) => {
                    let (mx, my) = ui.mouse();
                    let (sx, sy) = ui.screen_position(me);
                    self.deliver(
                        PointerKind::Cancelled,
                        mx as f32 - sx,
                        my as f32 - sy,
                        button,
                        0,
                        0,
                    );
                    true
                }
                None => false,
            },
            // The root's reading of a press and release the plugin saw.
            kind::CLICK | kind::DOUBLE_CLICK | kind::RIGHT_CLICK => self.canvas.takes_input(),
            _ => false,
        }
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        rc.push_clip(0.0, 0.0, w, h);
        let mut depth = 0usize;
        for command in self.canvas.paint.iter() {
            match command {
                PaintCmd::Clear(c) => rc.fill(0.0, 0.0, w, h, color(*c)),
                PaintCmd::FillRect { rect, color: c } => {
                    rc.fill(rect.x, rect.y, rect.w, rect.h, color(*c))
                }
                PaintCmd::StrokeRect {
                    rect,
                    color: c,
                    thickness,
                } => rc.rect_outline(rect.x, rect.y, rect.w, rect.h, color(*c), *thickness),
                PaintCmd::Line {
                    from,
                    to,
                    color: c,
                    thickness,
                } => rc.line(from.0, from.1, to.0, to.1, color(*c), *thickness),
                PaintCmd::Text {
                    text,
                    at,
                    color: c,
                    outline,
                } => {
                    if let Some(font) = self.font.as_ref() {
                        rc.string_dat(
                            font,
                            text,
                            at.0,
                            at.1,
                            color(*c),
                            *outline,
                            DEFAULT_OUTLINE_COLOR,
                        );
                    }
                }
                PaintCmd::Image { image, rect, tint } => {
                    if let Some((texture, _, _)) = self.images.get(image) {
                        rc.sprite(
                            *texture,
                            rect.x,
                            rect.y,
                            rect.w,
                            rect.h,
                            (0.0, 0.0, 1.0, 1.0),
                            color(*tint),
                        );
                    }
                }
                PaintCmd::ImageTransformed {
                    image,
                    rect,
                    tint,
                    rotation,
                    pivot,
                    scale_x,
                    scale_y,
                } => {
                    if let Some((texture, _, _)) = self.images.get(image) {
                        rc.sprite_transformed(
                            *texture,
                            rect.x,
                            rect.y,
                            rect.w,
                            rect.h,
                            (0.0, 0.0, 1.0, 1.0),
                            color(*tint),
                            *rotation,
                            (*scale_x, *scale_y),
                            *pivot,
                        );
                    }
                }
                PaintCmd::PushClip(rect) => {
                    rc.push_clip(rect.x, rect.y, rect.w, rect.h);
                    depth += 1;
                }
                PaintCmd::PopClip => {
                    if depth > 0 {
                        rc.pop_clip();
                        depth -= 1;
                    }
                }
            }
        }
        for _ in 0..depth {
            rc.pop_clip();
        }
        rc.pop_clip();
    }
}

/// Every canvas, on one layer of the overlay band.
#[derive(Debug)]
pub struct CanvasLayer {
    layer: ElementId,
    elements: BTreeMap<u32, ElementId>,
}

impl CanvasLayer {
    /// Takes a layer of the band.
    pub fn mount(ui: &mut UiRoot, host: &mut OverlayHost) -> Self {
        Self {
            layer: host.add_layer(ui, LAYER_NAME),
            elements: BTreeMap::new(),
        }
    }

    /// The layer.
    #[must_use]
    pub fn layer(&self) -> ElementId {
        self.layer
    }

    /// The element of canvas `id`.
    #[must_use]
    pub fn element(&self, id: u32) -> Option<ElementId> {
        self.elements.get(&id).copied()
    }

    /// One frame: the canvases as the copy has them, placed on the band's
    /// viewport, with their plugins' images (`image`: a plugin's handle to
    /// its texture).
    pub fn sync(
        &mut self,
        ui: &mut UiRoot,
        host: &OverlayHost,
        view: &PluginUiView,
        image: &mut dyn FnMut(&str, u32) -> Option<Icon>,
        font: Option<&Arc<UiFont>>,
        outbox: &PluginOutbox,
    ) {
        let gone: Vec<u32> = self
            .elements
            .keys()
            .copied()
            .filter(|id| view.canvas(*id).is_none())
            .collect();
        for id in gone {
            if let Some(el) = self.elements.remove(&id) {
                if let Some(w) = ui.widget_mut::<CanvasWidget>(el)
                    && let Some(button) = w.held.take()
                {
                    w.deliver(PointerKind::Cancelled, 0.0, 0.0, button, 0, 0);
                }
                if ui.captured() == Some(el) {
                    ui.release_capture();
                }
                ui.destroy(el);
            }
        }
        let viewport = host.viewport();
        for canvas in &view.canvases {
            let size = (
                canvas.descriptor.width as f32,
                canvas.descriptor.height as f32,
            );
            let (x, y) = canvas_origin(canvas.anchor, canvas.offset, size, viewport);
            let place = CanvasPlace {
                origin: (x, y),
                layer: viewport,
            };
            let el = *self.elements.entry(canvas.id).or_insert_with(|| {
                let mut e = Element::at(x, y, size.0, size.1).with_anchors(AnchorEdges::NONE);
                e.name = Some(format!(
                    "PluginCanvas:{}:{}",
                    canvas.owner.id, canvas.descriptor.canvas_id
                ));
                e.captures_pointer_drag = true;
                let el = ui.insert(
                    e,
                    CanvasWidget {
                        canvas: canvas.clone(),
                        images: HashMap::new(),
                        font: font.cloned(),
                        outbox: std::rc::Rc::clone(outbox),
                        held: None,
                        releases_seen: canvas.release_count,
                        place,
                    },
                );
                OverlayHost::add_to_layer(ui, self.layer, el, true);
                el
            });
            let images: HashMap<u32, Icon> = view
                .images
                .values()
                .filter(|i| i.owner == canvas.owner.id)
                .filter_map(|i| image(&i.owner, i.handle).map(|icon| (i.handle, icon)))
                .collect();
            let mut release = false;
            if let Some(w) = ui.widget_mut::<CanvasWidget>(el) {
                w.canvas = canvas.clone();
                w.images = images;
                w.place = place;
                if w.releases_seen != canvas.release_count {
                    w.releases_seen = canvas.release_count;
                    // The plugin let go: no cancel follows.
                    release = w.held.take().is_some();
                }
            }
            if release && ui.captured() == Some(el) {
                ui.release_capture();
            }
            let takes_input = canvas.takes_input();
            if let Some(e) = ui.element_mut(el) {
                e.left = x;
                e.top = y;
                e.width = size.0;
                e.height = size.1;
                e.click_through = !takes_input;
            }
            ui.set_visible(el, canvas.visible);
        }
        ui.set_visible(self.layer, !view.canvases.is_empty());
    }
}
