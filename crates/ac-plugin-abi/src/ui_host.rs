//! The plugin UI imports carried out (feature `ui-host`): each request
//! decoded, done on a session's [`PluginUi`] (which the client's runtime
//! owns, or the SDK's fake host), and the answer encoded. Also the other
//! direction: the session's UI events for a plugin as the plugin's
//! [`Event::Ui`]. One implementation for the client and the fake host, so a
//! plugin's native tests meet the same answers.

use crate::ui::{
    self as abi, ActionValue, BindingKind, CanvasChange, CanvasPaint, CanvasRegistration,
    CanvasSet, CanvasState, ClientWindowRequest, ControlLabel, ControlRef, ControlVisible,
    EncodedImage, HotkeyRebind, HotkeyRegistration, HotkeyState, ImageInfo, KeyChord, LineSet,
    PanelRegistration, PanelValues, PointerEvent, RgbaImage, TextSize, UiAction, UiValue, ViewShow,
};
use crate::{DecodeError, Event, Import, Wire};
use ac_plugin_ui::{
    self as ui, CanvasAnchor, CanvasDescriptor, ClientWindow, ClientWindowAction, Color8, MapPoint,
    PaintCmd, PaintRect, PanelDescriptor, PluginUi, PointerKind, UiOwner,
};

/// Whether `import` is one of the UI's.
#[must_use]
pub fn is_ui(import: Import) -> bool {
    import.name().starts_with("ui_")
}

fn kind(kind: BindingKind) -> ui::BindingKind {
    match kind {
        BindingKind::Bool => ui::BindingKind::Bool,
        BindingKind::Int => ui::BindingKind::Int,
        BindingKind::Uint => ui::BindingKind::Uint,
        BindingKind::Float => ui::BindingKind::Float,
        BindingKind::Text => ui::BindingKind::Text,
        BindingKind::TextList => ui::BindingKind::TextList,
        BindingKind::UintList => ui::BindingKind::UintList,
        BindingKind::IntList => ui::BindingKind::IntList,
        BindingKind::BoolList => ui::BindingKind::BoolList,
        BindingKind::Action => ui::BindingKind::Action,
        BindingKind::IntAction => ui::BindingKind::IntAction,
        BindingKind::FloatAction => ui::BindingKind::FloatAction,
        BindingKind::TextAction => ui::BindingKind::TextAction,
    }
}

fn value(value: UiValue) -> ui::UiValue {
    match value {
        UiValue::Bool(v) => ui::UiValue::Bool(v),
        UiValue::Int(v) => ui::UiValue::Int(v),
        UiValue::Uint(v) => ui::UiValue::Uint(v),
        UiValue::Float(v) => ui::UiValue::Float(v),
        UiValue::Text(v) => ui::UiValue::Text(v),
        UiValue::TextList(v) => ui::UiValue::TextList(v),
        UiValue::UintList(v) => ui::UiValue::UintList(v),
        UiValue::IntList(v) => ui::UiValue::IntList(v),
        UiValue::BoolList(v) => ui::UiValue::BoolList(v),
    }
}

fn typed_kind(kind: abi::TypedBindingKind) -> ui::BindingKind {
    use abi::TypedBindingKind as T;
    match kind {
        T::Bool => ui::BindingKind::Bool,
        T::Int => ui::BindingKind::Int,
        T::Uint => ui::BindingKind::Uint,
        T::Float => ui::BindingKind::Float,
        T::Text => ui::BindingKind::Text,
        T::TextList => ui::BindingKind::TextList,
        T::UintList => ui::BindingKind::UintList,
        T::IntList => ui::BindingKind::IntList,
        T::BoolList => ui::BindingKind::BoolList,
        T::Action => ui::BindingKind::Action,
        T::IntAction => ui::BindingKind::IntAction,
        T::FloatAction => ui::BindingKind::FloatAction,
        T::TextAction => ui::BindingKind::TextAction,
        T::OptionalBool => ui::BindingKind::OptionalBool,
        T::FloatList => ui::BindingKind::FloatList,
        T::Items => ui::BindingKind::Items,
        T::BoolAction => ui::BindingKind::BoolAction,
        T::UintAction => ui::BindingKind::UintAction,
        T::IntBoolAction => ui::BindingKind::IntBoolAction,
        T::TextBoolAction => ui::BindingKind::TextBoolAction,
        T::FloatListAction => ui::BindingKind::FloatListAction,
        T::IntListAction => ui::BindingKind::IntListAction,
    }
}

/// A typed registration's schema: the window's names, each items list
/// with its item type (an item type's list path is `Outer/Inner` for a
/// list an item of `Outer` carries).
fn typed_schema(
    binding_type: String,
    bindings: Vec<abi::TypedBindingDecl>,
    item_types: &[abi::ItemTypeDecl],
) -> ui::BindingSchema {
    fn fill(
        schema: &mut ui::BindingSchema,
        path: &str,
        bindings: Vec<abi::TypedBindingDecl>,
        item_types: &[abi::ItemTypeDecl],
    ) {
        for decl in bindings {
            let kind = typed_kind(decl.kind);
            if kind == ui::BindingKind::Items {
                let list = if path.is_empty() {
                    decl.name.clone()
                } else {
                    format!("{path}/{}", decl.name)
                };
                let item = item_types.iter().find(|t| t.list == list);
                let mut inner = ui::BindingSchema::new(
                    item.map_or_else(|| "Item".to_owned(), |t| t.type_name.clone()),
                );
                if let Some(item) = item {
                    fill(&mut inner, &list, item.bindings.clone(), item_types);
                }
                schema.declare_items(decl.name, inner);
            } else {
                schema.declare(decl.name, kind);
            }
        }
    }
    let mut schema = ui::BindingSchema::new(binding_type);
    fill(&mut schema, "", bindings, item_types);
    schema
}

fn item_value(value: abi::ItemValue) -> ui::UiValue {
    use abi::ItemValue as I;
    match value {
        I::Bool(v) => ui::UiValue::Bool(v),
        I::Int(v) => ui::UiValue::Int(v),
        I::Uint(v) => ui::UiValue::Uint(v),
        I::Float(v) => ui::UiValue::Float(v),
        I::Text(v) => ui::UiValue::Text(v),
        I::TextList(v) => ui::UiValue::TextList(v),
        I::UintList(v) => ui::UiValue::UintList(v),
        I::IntList(v) => ui::UiValue::IntList(v),
        I::BoolList(v) => ui::UiValue::BoolList(v),
        I::OptionalBool(v) => ui::UiValue::OptionalBool(v),
        I::FloatList(v) => ui::UiValue::FloatList(v),
    }
}

/// Item rows off the wire: each row's nested rows gathered under it, by the
/// list they name.
///
/// # Errors
/// A row whose parent is not an earlier row.
fn item_rows(rows: abi::ItemRows) -> Result<std::sync::Arc<Vec<ui::ItemRecord>>, String> {
    let rows = rows.rows;
    let count = rows.len();
    let mut records: Vec<ui::ItemRecord> = Vec::with_capacity(count);
    let mut parents: Vec<(i32, String)> = Vec::with_capacity(count);
    for (index, row) in rows.into_iter().enumerate() {
        if row.parent >= 0 && usize::try_from(row.parent).is_ok_and(|p| p >= index) {
            return Err(format!(
                "item row {index} names parent {} that does not come before it",
                row.parent
            ));
        }
        let mut record = ui::ItemRecord::new();
        for value in row.values {
            record.values.insert(value.name, item_value(value.value));
        }
        records.push(record);
        parents.push((row.parent, row.list));
    }
    // Children last to first, so each list keeps its order when gathered.
    let mut nested: Vec<Vec<(String, ui::ItemRecord)>> = vec![Vec::new(); count];
    let mut top = Vec::new();
    for (index, record) in records.into_iter().enumerate().rev() {
        let mut record = record;
        let mut lists: std::collections::BTreeMap<String, Vec<ui::ItemRecord>> =
            std::collections::BTreeMap::new();
        for (list, child) in std::mem::take(&mut nested[index]).into_iter().rev() {
            lists.entry(list).or_default().push(child);
        }
        for (list, children) in lists {
            record
                .values
                .insert(list, ui::UiValue::Items(std::sync::Arc::new(children)));
        }
        let (parent, list) = &parents[index];
        match usize::try_from(*parent) {
            Ok(p) => nested[p].push((list.clone(), record)),
            Err(_) => top.push(record),
        }
    }
    top.reverse();
    Ok(std::sync::Arc::new(top))
}

fn typed_value(value: abi::TypedValue) -> Result<ui::UiValue, String> {
    use abi::TypedValue as T;
    Ok(match value {
        T::Bool(v) => ui::UiValue::Bool(v),
        T::Int(v) => ui::UiValue::Int(v),
        T::Uint(v) => ui::UiValue::Uint(v),
        T::Float(v) => ui::UiValue::Float(v),
        T::Text(v) => ui::UiValue::Text(v),
        T::TextList(v) => ui::UiValue::TextList(v),
        T::UintList(v) => ui::UiValue::UintList(v),
        T::IntList(v) => ui::UiValue::IntList(v),
        T::BoolList(v) => ui::UiValue::BoolList(v),
        T::OptionalBool(v) => ui::UiValue::OptionalBool(v),
        T::FloatList(v) => ui::UiValue::FloatList(v),
        T::Items(rows) => ui::UiValue::Items(item_rows(rows)?),
    })
}

fn color(c: abi::Color) -> Color8 {
    Color8::new(c.r, c.g, c.b, c.a)
}

fn abi_color(c: Color8) -> abi::Color {
    abi::Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// A theme as plugins read it: its kind, the picker's name for it and its
/// colours (OpenAC `PluginUiThemeSettings.ThemeInfo`).
#[must_use]
pub fn theme_info(kind: ui::ThemeKind) -> abi::ThemeInfo {
    abi::ThemeInfo {
        kind: match kind {
            ui::ThemeKind::Classic => abi::ThemeKind::Classic,
            ui::ThemeKind::Moss => abi::ThemeKind::Moss,
            ui::ThemeKind::Brass => abi::ThemeKind::Brass,
        },
        display_name: kind.display_name().to_owned(),
        palette: kind.palette().map(|p| abi::ThemePalette {
            background: abi_color(p.background),
            field: abi_color(p.field),
            border: abi_color(p.border),
            text: abi_color(p.text),
            muted: abi_color(p.muted),
            accent: abi_color(p.accent),
            selected: abi_color(p.selected),
        }),
    }
}

fn rect(r: abi::Rect) -> PaintRect {
    PaintRect {
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
    }
}

fn paint(command: abi::PaintCmd) -> PaintCmd {
    match command {
        abi::PaintCmd::Clear(c) => PaintCmd::Clear(color(c)),
        abi::PaintCmd::FillRect(r, c) => PaintCmd::FillRect {
            rect: rect(r),
            color: color(c),
        },
        abi::PaintCmd::StrokeRect(r, c, thickness) => PaintCmd::StrokeRect {
            rect: rect(r),
            color: color(c),
            thickness,
        },
        abi::PaintCmd::Line(x0, y0, x1, y1, c, thickness) => PaintCmd::Line {
            from: (x0, y0),
            to: (x1, y1),
            color: color(c),
            thickness,
        },
        abi::PaintCmd::Text(text, x, y, c, outline) => PaintCmd::Text {
            text,
            at: (x, y),
            color: color(c),
            outline,
        },
        abi::PaintCmd::Image(image, r, tint) => PaintCmd::Image {
            image,
            rect: rect(r),
            tint: color(tint),
        },
        abi::PaintCmd::ImageTransformed(image, r, tint, rotation, px, py, sx, sy) => {
            PaintCmd::ImageTransformed {
                image,
                rect: rect(r),
                tint: color(tint),
                rotation,
                pivot: (px, py),
                scale_x: sx,
                scale_y: sy,
            }
        }
        abi::PaintCmd::PushClip(r) => PaintCmd::PushClip(rect(r)),
        abi::PaintCmd::PopClip => PaintCmd::PopClip,
    }
}

fn point(p: abi::MapPoint) -> MapPoint {
    MapPoint {
        east_west: p.east_west,
        north_south: p.north_south,
        elevation: p.elevation,
    }
}

fn image_info((handle, width, height): (u32, u32, u32)) -> Vec<u8> {
    ImageInfo {
        handle,
        width,
        height,
    }
    .to_bytes()
}

/// A fact about the world a UI call needs from the session before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldQuery {
    /// The spell's icon render surface, if the client knows the spell.
    SpellIcon(u32),
    /// The object's icon, if the client knows the object.
    ObjectIcon(u32),
}

/// What the session must look up before [`handle`] carries out `import`
/// (answer it, then pass the answer to [`handle`]).
#[must_use]
pub fn world_query(import: Import, request: &[u8]) -> Option<WorldQuery> {
    match import {
        Import::UiImageSpellIcon => u32::from_bytes(request).ok().map(WorldQuery::SpellIcon),
        Import::UiImageObjectIcon => u32::from_bytes(request).ok().map(WorldQuery::ObjectIcon),
        _ => None,
    }
}

/// Carries out UI `import` for plugin `owner` on `state`; `answer` is the
/// session's answer to the call's [`world_query`], if it has one.
///
/// # Errors
/// A request that does not decode, or an import that is not a UI call (the
/// contract is broken; the plugin is stopped).
pub fn handle(
    import: Import,
    request: &[u8],
    owner: &UiOwner,
    state: &mut PluginUi,
    answer: Option<u32>,
) -> Result<Vec<u8>, String> {
    let bad =
        |error: DecodeError| format!("its request to {} is malformed: {error}", import.name());
    let id = owner.id.as_str();
    let reply = match import {
        Import::UiPanelRegister => {
            let r = PanelRegistration::from_bytes(request).map_err(bad)?;
            let mut schema = ui::BindingSchema::new(r.binding_type);
            for decl in r.bindings {
                schema.declare(decl.name, kind(decl.kind));
            }
            let descriptor = PanelDescriptor {
                window_id: r.window_id,
                title: r.title,
                icon_text: r.icon_text,
                icon_surface_id: r.icon_surface_id,
                start_visible: r.start_visible,
                show_in_side_panel: r.show_in_side_panel,
            };
            state
                .register_panel(owner, descriptor, &r.markup, schema)
                .to_bytes()
        }
        Import::UiPanelRemove => {
            let panel = u32::from_bytes(request).map_err(bad)?;
            state.remove_panel(id, panel).to_bytes()
        }
        Import::UiPanelSet => {
            let r = PanelValues::from_bytes(request).map_err(bad)?;
            let values = r
                .values
                .into_iter()
                .map(|v| (v.name, value(v.value)))
                .collect();
            state.set_values(id, r.panel, values).to_bytes()
        }
        Import::UiPanelRegisterTyped => {
            let r = abi::TypedPanelRegistration::from_bytes(request).map_err(bad)?;
            let schema = typed_schema(r.binding_type, r.bindings, &r.item_types);
            let descriptor = PanelDescriptor {
                window_id: r.window_id,
                title: r.title,
                icon_text: r.icon_text,
                icon_surface_id: r.icon_surface_id,
                start_visible: r.start_visible,
                show_in_side_panel: r.show_in_side_panel,
            };
            state
                .register_panel(owner, descriptor, &r.markup, schema)
                .to_bytes()
        }
        Import::UiPanelSetTyped => {
            let r = abi::TypedPanelValues::from_bytes(request).map_err(bad)?;
            let mut values = Vec::with_capacity(r.values.len());
            for v in r.values {
                match typed_value(v.value) {
                    Ok(value) => values.push((v.name, value)),
                    Err(why) => return Ok(Err::<(), String>(why).to_bytes()),
                }
            }
            state.set_values(id, r.panel, values).to_bytes()
        }
        Import::UiDialogConfirm => {
            let r = abi::DialogRequest::from_bytes(request).map_err(bad)?;
            state
                .confirm(
                    owner,
                    &r.title,
                    &r.message,
                    r.buttons,
                    r.owner_view.as_deref(),
                )
                .to_bytes()
        }
        Import::UiDialogWithdraw => {
            let handle = u32::from_bytes(request).map_err(bad)?;
            state.withdraw_dialog(id, handle).to_bytes()
        }
        Import::UiPanelShowModal => {
            let r = abi::ModalShow::from_bytes(request).map_err(bad)?;
            state
                .show_modal(id, &r.view, r.owner_view.as_deref())
                .to_bytes()
        }
        Import::UiPanelShow => {
            let r = ViewShow::from_bytes(request).map_err(bad)?;
            state.show_panel(id, &r.view, r.show).to_bytes()
        }
        Import::UiViewExists => {
            let view = String::from_bytes(request).map_err(bad)?;
            state.view_exists(id, &view).to_bytes()
        }
        Import::UiViewVisible => {
            let view = String::from_bytes(request).map_err(bad)?;
            state.is_view_visible(id, &view).to_bytes()
        }
        Import::UiControlExists => {
            let r = ControlRef::from_bytes(request).map_err(bad)?;
            state.control_exists(id, &r.view, &r.control).to_bytes()
        }
        Import::UiControlLabel => {
            let r = ControlLabel::from_bytes(request).map_err(bad)?;
            state
                .set_control_label(id, &r.view, &r.control, &r.label)
                .to_bytes()
        }
        Import::UiControlVisible => {
            let r = ControlVisible::from_bytes(request).map_err(bad)?;
            state
                .set_control_visible(id, &r.view, &r.control, r.visible)
                .to_bytes()
        }
        Import::UiClientWindow => {
            let r = ClientWindowRequest::from_bytes(request).map_err(bad)?;
            let done = ClientWindow::from_index(r.window).is_some_and(|window| match r.action {
                0 => state.client_window(window, ClientWindowAction::Toggle),
                1 => state.client_window(window, ClientWindowAction::Show),
                2 => state.client_window(window, ClientWindowAction::Hide),
                3 => state.is_client_window_visible(window),
                _ => false,
            });
            done.to_bytes()
        }
        Import::UiCanvasRegister => {
            let r = CanvasRegistration::from_bytes(request).map_err(bad)?;
            let Some(anchor) = CanvasAnchor::from_index(r.anchor) else {
                return Ok(
                    Err::<u32, String>(format!("canvas anchor {} is not 0 to 8", r.anchor))
                        .to_bytes(),
                );
            };
            let descriptor = CanvasDescriptor {
                canvas_id: r.canvas_id,
                width: r.width,
                height: r.height,
                anchor,
                offset: (r.offset_x, r.offset_y),
                start_visible: r.start_visible,
                accepts_pointer_input: r.accepts_pointer_input,
            };
            state.register_canvas(owner, descriptor).to_bytes()
        }
        Import::UiCanvasPaint => {
            let r = CanvasPaint::from_bytes(request).map_err(bad)?;
            let commands = r.commands.into_iter().map(paint).collect();
            state.paint_canvas(id, r.canvas, commands).to_bytes()
        }
        Import::UiCanvasSet => {
            let r = CanvasSet::from_bytes(request).map_err(bad)?;
            let done = match r.change {
                CanvasChange::Visible(v) => state.place_canvas(id, r.canvas, Some(v), None, None),
                CanvasChange::Anchor(a) => match CanvasAnchor::from_index(a) {
                    Some(a) => state.place_canvas(id, r.canvas, None, Some(a), None),
                    None => false,
                },
                CanvasChange::Offset(x, y) => {
                    state.place_canvas(id, r.canvas, None, None, Some((x, y)))
                }
                CanvasChange::Listen(l) => state.listen_canvas(id, r.canvas, l),
                CanvasChange::ReleasePointer => state.release_canvas_pointer(id, r.canvas),
            };
            done.to_bytes()
        }
        Import::UiCanvasState => {
            let canvas = u32::from_bytes(request).map_err(bad)?;
            state
                .canvas(id, canvas)
                .map(|c| CanvasState {
                    width: c.descriptor.width,
                    height: c.descriptor.height,
                    visible: c.visible,
                    anchor: c.anchor.index(),
                    offset_x: c.offset.0,
                    offset_y: c.offset.1,
                    listening: c.listening,
                })
                .to_bytes()
        }
        Import::UiCanvasAvailable => {
            let canvas = u32::from_bytes(request).map_err(bad)?;
            state.canvas_available(id, canvas).to_bytes()
        }
        Import::UiCanvasRemove => {
            let canvas = u32::from_bytes(request).map_err(bad)?;
            state.remove_canvas(id, canvas).to_bytes()
        }
        Import::UiMeasureText => {
            let text = String::from_bytes(request).map_err(bad)?;
            let (width, height) = state.measure_text(&text);
            TextSize { width, height }.to_bytes()
        }
        Import::UiTheme => {
            <()>::from_bytes(request).map_err(bad)?;
            theme_info(state.theme()).to_bytes()
        }
        Import::UiImageClientArt => {
            let surface = u32::from_bytes(request).map_err(bad)?;
            image_info(state.image_client_art(id, surface))
        }
        Import::UiImageSpellIcon => {
            let spell = u32::from_bytes(request).map_err(bad)?;
            image_info(state.image_spell_icon(id, spell, answer))
        }
        Import::UiImageObjectIcon => {
            let object = u32::from_bytes(request).map_err(bad)?;
            image_info(state.image_object_icon(id, object, answer.is_some()))
        }
        Import::UiImageRgba => {
            let r = RgbaImage::from_bytes(request).map_err(bad)?;
            image_info(state.image_rgba(id, &r.name, r.width, r.height, r.rgba))
        }
        Import::UiImageDecode => {
            let e = EncodedImage::from_bytes(request).map_err(bad)?;
            image_info(state.image_decode(id, &e.name, &e.bytes))
        }
        Import::UiImageRelease => {
            let handle = u32::from_bytes(request).map_err(bad)?;
            state.image_release(id, handle).to_bytes()
        }
        Import::UiImageCount => u32::try_from(state.image_count(id))
            .unwrap_or(u32::MAX)
            .to_bytes(),
        Import::UiLabelsShow => {
            let labels = Vec::<abi::WorldLabel>::from_bytes(request).map_err(bad)?;
            let labels = labels
                .into_iter()
                .map(|l| ui::WorldLabel {
                    object_id: l.object_id,
                    text: l.text,
                    color: [l.r, l.g, l.b, l.a],
                    height_offset: l.height_offset,
                    line: l.line,
                    max_range: l.max_range,
                    outline: l.outline,
                })
                .collect();
            state.show_labels(id, labels).to_bytes()
        }
        Import::UiLinesCreate => state.create_line_layer(id).to_bytes(),
        Import::UiLinesSet => {
            let r = LineSet::from_bytes(request).map_err(bad)?;
            let lines = r
                .lines
                .into_iter()
                .map(|l| ui::WorldLine {
                    start: point(l.start),
                    end: point(l.end),
                    color_rgb: l.color_rgb,
                    follow_terrain: l.follow_terrain,
                    width_metres: l.width_metres,
                })
                .collect();
            state.set_lines(id, r.layer, lines).to_bytes()
        }
        Import::UiLinesRemove => {
            let layer = u32::from_bytes(request).map_err(bad)?;
            state.remove_line_layer(id, layer).to_bytes()
        }
        Import::UiProjectileDebug => {
            let samples = Vec::<abi::ProjectileSample>::from_bytes(request).map_err(bad)?;
            state.show_projectile_samples(
                samples
                    .into_iter()
                    .map(|s| ui::ProjectileSample {
                        position: point(s.position),
                        is_clear: s.is_clear,
                        radius: s.radius,
                    })
                    .collect(),
            );
            Vec::new()
        }
        Import::UiHotkeyRegister => {
            let r = HotkeyRegistration::from_bytes(request).map_err(bad)?;
            state
                .register_hotkey(id, &r.id, &r.display_name, chord(&r.chord))
                .map(|s| hotkey_state(&s))
                .to_bytes()
        }
        Import::UiHotkeyUnregister => {
            let hotkey = String::from_bytes(request).map_err(bad)?;
            state.unregister_hotkey(id, &hotkey).to_bytes()
        }
        Import::UiHotkeyRebind => {
            let r = HotkeyRebind::from_bytes(request).map_err(bad)?;
            state
                .rebind_hotkey(id, &r.id, chord(&r.chord))
                .map(|s| hotkey_state(&s))
                .to_bytes()
        }
        Import::UiHotkeyState => {
            let hotkey = String::from_bytes(request).map_err(bad)?;
            state
                .hotkeys()
                .state(id, &hotkey)
                .map(|s| hotkey_state(&s))
                .to_bytes()
        }
        Import::UiInputActionsListen => {
            let listen = bool::from_bytes(request).map_err(bad)?;
            state.listen_input_actions(id, listen).to_bytes()
        }
        other => return Err(format!("{} is not a plugin UI call", other.name())),
    };
    Ok(reply)
}

/// The session's UI event as the plugin's event.
#[must_use]
pub fn event(event: ui::UiEvent) -> Event {
    Event::Ui(match event {
        ui::UiEvent::Action {
            panel,
            handler,
            value,
        } => abi::UiEvent::Action(UiAction {
            panel,
            handler,
            value: match value {
                ui::ActionValue::None => ActionValue::None,
                ui::ActionValue::Int(v) => ActionValue::Int(v),
                ui::ActionValue::Float(v) => ActionValue::Float(v),
                ui::ActionValue::Text(v) => ActionValue::Text(v),
                ui::ActionValue::Bool(v) => ActionValue::from_bool(v),
                ui::ActionValue::Uint(v) => ActionValue::from_uint(v),
                ui::ActionValue::IntBool(v, flag) => ActionValue::from_int_bool(v, flag),
                ui::ActionValue::TextBool(v, flag) => ActionValue::from_text_bool(&v, flag),
                ui::ActionValue::FloatList(v) => ActionValue::from_float_list(&v),
                ui::ActionValue::IntList(v) => ActionValue::from_int_list(&v),
            },
        }),
        ui::UiEvent::Pointer { canvas, event, .. } => abi::UiEvent::Pointer(PointerEvent {
            canvas,
            kind: match event.kind {
                PointerKind::Down => 0,
                PointerKind::Up => 1,
                PointerKind::Move => 2,
                PointerKind::Wheel => 3,
                PointerKind::Cancelled => 4,
            },
            x: event.x,
            y: event.y,
            button: event.button,
            modifiers: event.modifiers,
            wheel_delta: event.wheel_delta,
        }),
        ui::UiEvent::Hotkey(id) => abi::UiEvent::Hotkey(id),
        ui::UiEvent::InputAction { action, activation } => {
            abi::UiEvent::InputAction(action, activation)
        }
        ui::UiEvent::DialogAnswered { handle, button } => abi::UiEvent::Action(UiAction {
            panel: abi::DIALOG_PANEL,
            handler: format!("{}{handle}", abi::DIALOG_HANDLER_PREFIX),
            value: ActionValue::Int(button),
        }),
    })
}

/// A chord off the wire.
#[must_use]
pub fn chord(c: &KeyChord) -> ui::hotkeys::Chord {
    ui::hotkeys::Chord {
        key: c.key,
        ctrl: c.ctrl,
        alt: c.alt,
        shift: c.shift,
    }
}

/// A chord for the wire.
#[must_use]
pub fn key_chord(c: ui::hotkeys::Chord) -> KeyChord {
    KeyChord {
        key: c.key,
        ctrl: c.ctrl,
        alt: c.alt,
        shift: c.shift,
    }
}

fn hotkey_state(s: &ui::hotkeys::HotkeyState) -> HotkeyState {
    HotkeyState {
        is_bound: s.is_bound,
        effective: key_chord(s.effective),
    }
}

/// The UI's reports, as log lines for their plugins (OpenAC reports each
/// refusal once in the client's log: "Plugin '<id>': <message>.").
#[must_use]
pub fn reports(state: &mut PluginUi) -> Vec<(String, String)> {
    state
        .take_reports()
        .into_iter()
        .map(|(owner, message)| {
            let line = format!("Plugin '{owner}': {message}.");
            (owner, line)
        })
        .collect()
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    #[test]
    fn a_theme_reads_as_its_kind_name_and_palette_bytes() {
        // PluginThemeContractTests / PluginUiThemeSettings.ThemeInfo:
        // Classic has no palette; each channel is the byte the
        // palette was written with.
        let classic = theme_info(ui::ThemeKind::Classic);
        assert_eq!(classic.kind, abi::ThemeKind::Classic);
        assert_eq!(classic.display_name, "Classic");
        assert_eq!(classic.palette, None);
        let moss = theme_info(ui::ThemeKind::Moss);
        assert_eq!(moss.kind, abi::ThemeKind::Moss);
        assert_eq!(moss.display_name, "Charcoal + moss");
        assert_eq!(
            moss.palette.as_ref().map(|p| p.background.clone()),
            Some(abi::Color {
                r: 0x17,
                g: 0x1D,
                b: 0x1B,
                a: 255,
            })
        );
        assert_eq!(
            theme_info(ui::ThemeKind::Brass).display_name,
            "Warm graphite + brass"
        );
        // Over the wire and back.
        let bytes = moss.to_bytes();
        assert_eq!(abi::ThemeInfo::from_bytes(&bytes).unwrap(), moss);
    }

    #[test]
    fn the_key_names_are_the_same_list_on_both_sides() {
        // A `<hotkey>` box in the window spells keys with the UI's list;
        // a plugin reads them with the ABI's.
        let names: Vec<&str> = crate::keys::PluginKey::ALL
            .iter()
            .map(|k| k.name())
            .collect();
        assert_eq!(names, ui::hotkeys::KEY_NAMES);
    }

    #[test]
    fn newer_handler_arguments_read_back_as_sent() {
        for (value, read) in [
            (
                ui::ActionValue::Bool(true),
                abi::ActionValue::from_bool(true),
            ),
            (
                ui::ActionValue::Uint(0xFF00_0001),
                abi::ActionValue::from_uint(0xFF00_0001),
            ),
        ] {
            let Event::Ui(abi::UiEvent::Action(action)) = event(ui::UiEvent::Action {
                panel: 1,
                handler: "H".into(),
                value,
            }) else {
                panic!()
            };
            assert_eq!(action.value, read);
        }
        assert_eq!(abi::ActionValue::from_bool(true).as_bool(), Some(true));
        assert_eq!(
            abi::ActionValue::from_uint(0xFF00_0001).as_uint(),
            Some(0xFF00_0001)
        );
        assert_eq!(
            abi::ActionValue::from_int_bool(3, true).as_int_bool(),
            Some((3, true))
        );
        assert_eq!(
            abi::ActionValue::from_int_bool(-1, false).as_int_bool(),
            Some((-1, false))
        );
        assert_eq!(
            abi::ActionValue::from_text_bool("a,b/c", true).as_text_bool(),
            Some(("a,b/c".to_owned(), true))
        );
        assert_eq!(
            abi::ActionValue::from_float_list(&[120.5, 0.0, 1e-7]).as_float_list(),
            Some(vec![120.5, 0.0, 1e-7])
        );
        assert_eq!(
            abi::ActionValue::from_int_list(&[]).as_int_list(),
            Some(vec![])
        );
        assert_eq!(
            abi::ActionValue::from_int_list(&[2, 5]).as_int_list(),
            Some(vec![2, 5])
        );
        let Event::Ui(abi::UiEvent::Action(answer)) = event(ui::UiEvent::DialogAnswered {
            handle: 7,
            button: 2,
        }) else {
            panic!()
        };
        assert_eq!(answer.dialog_answer(), Some((7, 2)));
    }

    #[test]
    fn item_rows_gather_nested_lists_under_their_parents() {
        let row = |parent: i32, list: &str, label: &str| abi::ItemRow {
            parent,
            list: list.into(),
            values: vec![abi::NamedItemValue {
                name: "Label".into(),
                value: abi::ItemValue::Text(label.into()),
            }],
        };
        let rows = item_rows(abi::ItemRows {
            rows: vec![
                row(-1, "", "A"),
                row(0, "Kids", "A1"),
                row(0, "Kids", "A2"),
                row(-1, "", "B"),
                row(3, "Kids", "B1"),
            ],
        })
        .unwrap();
        let label = |r: &ui::ItemRecord| match r.values.get("Label") {
            Some(ui::UiValue::Text(t)) => t.clone(),
            _ => String::new(),
        };
        assert_eq!(rows.iter().map(label).collect::<Vec<_>>(), ["A", "B"]);
        let Some(ui::UiValue::Items(kids)) = rows[0].values.get("Kids") else {
            panic!()
        };
        assert_eq!(kids.iter().map(label).collect::<Vec<_>>(), ["A1", "A2"]);
        assert!(
            item_rows(abi::ItemRows {
                rows: vec![row(0, "Kids", "self")]
            })
            .is_err()
        );
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;

    /// The plugin reads a row handler as the window writes it.
    #[test]
    fn a_row_handler_reads_back_as_the_window_writes_it() {
        let text = format!(
            "{}{}SetOn",
            ui::row::step_text("Sections", 0, Some("a/b%")),
            ui::row::step_text("Rows", 7, None)
        );
        let action = abi::RowAction::parse(&text).unwrap();
        let (steps, name) = ui::row::split(&text).unwrap();
        assert_eq!(action.handler, name);
        assert_eq!(
            action
                .rows
                .iter()
                .map(|s| (s.list.clone(), s.index, s.key.clone()))
                .collect::<Vec<_>>(),
            steps
                .into_iter()
                .map(|s| (s.list, s.index, s.key))
                .collect::<Vec<_>>()
        );
        assert_eq!(action.rows[0].key.as_deref(), Some("a/b%"));
        assert_eq!(abi::RowAction::parse("SetOn"), None);
    }
}
