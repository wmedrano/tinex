//! Declarative widgets with retained interaction state and queued application actions.
//! Layout, drawing, and hit testing share logical-coordinate geometry.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use vello::Scene;
use vello::kurbo::{Affine, Point, Rect, Size, Stroke};
use vello::peniko::{Color, Fill};

use crate::{LevelMeter, MeterStyle, TextRenderer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId(u64);

impl WidgetId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for WidgetId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Tone {
    #[default]
    Normal,
    Muted,
    Error,
}

/// A discrete horizontal slider. Bounds and step must be finite, with min < max
/// and step > 0. Values and pointer positions are clamped and rounded to the step.
#[derive(Clone)]
pub struct Slider<A> {
    pub id: WidgetId,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub width: f64,
    pub enabled: bool,
    pub on_change: Arc<dyn Fn(f32) -> A>,
}

impl<A> Slider<A> {
    fn quantize(&self, value: f32) -> f32 {
        if !value.is_finite() {
            return self.min;
        }
        (self.min + ((value.clamp(self.min, self.max) - self.min) / self.step).round() * self.step)
            .clamp(self.min, self.max)
    }

    fn valid(&self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min < self.max
            && self.step.is_finite()
            && self.step > 0.0
    }
}

/// The initial layout vocabulary: fixed-size buttons, fixed or flexible labels, rows,
/// clickable rows, columns, and vertical scroll regions. No application/engine dependencies.
pub enum Widget<A> {
    Slider(Slider<A>),
    /// Live dBFS meter behind a gain slider, with a readout below the rail.
    /// The meter shares the slider's dB range and uses its existing color zones.
    MeterSlider {
        slider: Slider<A>,
        meter: LevelMeter,
        readout: String,
    },
    Button {
        id: WidgetId,
        label: String,
        width: f64,
        enabled: bool,
        action: A,
    },
    Label {
        text: String,
        tone: Tone,
        width: Option<f64>,
    },
    LevelMeter {
        meter: LevelMeter,
        width: f64,
    },
    CompactLevelMeter {
        meter: LevelMeter,
        width: f64,
    },
    /// Noninteractive linear meter, clamped to the range 0–100%.
    PercentageMeter {
        percentage: f64,
        width: f64,
    },
    /// Compact, square-edged footer spanning the window width.
    StatusBar(Vec<Self>),
    Row {
        children: Vec<Self>,
        surface: bool,
    },
    /// A focusable row surface. Nested controls take precedence over its action,
    /// including disabled buttons, which block activation in their own bounds.
    ClickableRow {
        id: WidgetId,
        children: Vec<Self>,
        enabled: bool,
        action: A,
    },
    Column(Vec<Self>),
    Scroll {
        id: WidgetId,
        child: Box<Self>,
    },
}

impl<A> Widget<A> {
    pub fn button(
        id: WidgetId,
        label: impl Into<String>,
        width: f64,
        enabled: bool,
        action: A,
    ) -> Self {
        Self::Button {
            id,
            label: label.into(),
            width,
            enabled,
            action,
        }
    }

    pub fn clickable_row(id: WidgetId, children: Vec<Self>, enabled: bool, action: A) -> Self {
        Self::ClickableRow {
            id,
            children,
            enabled,
            action,
        }
    }

    pub fn label(text: impl Into<String>, tone: Tone) -> Self {
        Self::Label {
            text: text.into(),
            tone,
            width: None,
        }
    }

    pub fn fixed_label(text: impl Into<String>, tone: Tone, width: f64) -> Self {
        Self::Label {
            text: text.into(),
            tone,
            width: Some(width),
        }
    }

    pub fn level_meter(amplitude: f32, width: f64) -> Self {
        Self::LevelMeter {
            meter: LevelMeter::new(amplitude),
            width,
        }
    }

    pub fn compact_level_meter(amplitude: f32, width: f64) -> Self {
        Self::CompactLevelMeter {
            meter: LevelMeter::new(amplitude),
            width,
        }
    }

    pub fn percentage_meter(percentage: f64, width: f64) -> Self {
        Self::PercentageMeter {
            percentage: if percentage.is_nan() {
                0.0
            } else {
                percentage.clamp(0.0, 100.0)
            },
            width,
        }
    }

    fn height(&self) -> f64 {
        match self {
            Self::Button { .. } | Self::Slider(_) => 36.0,
            Self::MeterSlider { .. } => 60.0,
            Self::Label { .. } => 28.0,
            Self::LevelMeter { .. } => LevelMeter::HEIGHT,
            Self::CompactLevelMeter { .. } | Self::PercentageMeter { .. } => 28.0,
            Self::StatusBar(_) => 32.0,
            Self::Row { children, .. } | Self::ClickableRow { children, .. } => {
                children.iter().map(Self::height).fold(0.0, f64::max) + 16.0
            }
            Self::Column(children) => {
                children.iter().map(Self::height).sum::<f64>()
                    + 8.0 * children.len().saturating_sub(1) as f64
            }
            Self::Scroll { .. } => 0.0,
        }
    }

    fn fixed_width(&self) -> Option<f64> {
        match self {
            Self::Slider(slider) | Self::MeterSlider { slider, .. } => Some(slider.width),
            Self::Button { width, .. }
            | Self::LevelMeter { width, .. }
            | Self::CompactLevelMeter { width, .. }
            | Self::PercentageMeter { width, .. } => Some(*width),
            Self::Label { width, .. } => *width,
            _ => None,
        }
    }
}

pub struct WidgetStyle {
    pub text: Color,
    pub muted: Color,
    pub error: Color,
    pub surface: Color,
    pub button: Color,
    pub hovered: Color,
    pub pressed: Color,
    pub disabled: Color,
    pub focus: Color,
    pub meter: MeterStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationKey {
    Enter,
    Space,
}

#[derive(Clone, Copy, Debug)]
pub enum AdjustmentKey {
    Decrease,
    Increase,
    Home,
    End,
}

pub enum Input {
    Adjust(AdjustmentKey),
    PointerMoved(Point),
    PointerLeft,
    PointerPressed,
    PointerReleased,
    Scroll(f64), // Positive values move down, measured in logical pixels.
    Tab {
        backwards: bool,
    },
    Key {
        key: ActivationKey,
        pressed: bool,
        repeat: bool,
    },
    Cancel,
}

struct Item<A> {
    rect: Rect,
    clip: Rect,
    scroll: Option<WidgetId>,
    kind: ItemKind<A>,
}

enum ItemKind<A> {
    Slider {
        slider: Slider<A>,
        meter: Option<LevelMeter>,
        readout: Option<String>,
    },
    Surface,
    StatusSurface,
    PercentageMeter(f64),
    CompactLevelMeter(LevelMeter),
    LevelMeter(LevelMeter),
    Label {
        text: String,
        tone: Tone,
    },
    Button {
        id: WidgetId,
        // Clickable rows share button interaction, but render their children
        // instead of a label, with a surface-colored idle background.
        label: Option<String>,
        enabled: bool,
        action: A,
    },
}

struct ScrollRegion {
    id: WidgetId,
    rect: Rect,
    clip: Rect,
    max_offset: f64,
}

/// Owns only the current declarative view and transient UI state. Widgets emit
/// cloned semantic actions; callers dispatch them after input traversal ends.
pub struct Ui<A> {
    root: Widget<A>,
    size: Size,
    items: Vec<Item<A>>,
    regions: Vec<ScrollRegion>,
    offsets: HashMap<WidgetId, f64>,
    pointer: Option<Point>,
    hovered: Option<WidgetId>,
    captured: Option<WidgetId>,
    focus: Option<WidgetId>,
    key_pressed: Option<(WidgetId, ActivationKey)>,
}

impl<A: Clone> Ui<A> {
    pub fn new(root: Widget<A>, size: Size) -> Self {
        let mut ui = Self {
            root,
            size,
            items: Vec::new(),
            regions: Vec::new(),
            offsets: HashMap::new(),
            pointer: None,
            hovered: None,
            captured: None,
            focus: None,
            key_pressed: None,
        };
        ui.layout();
        ui
    }

    pub fn set_view(&mut self, root: Widget<A>, size: Size) {
        self.root = root;
        self.size = size;
        self.layout();
    }

    fn layout(&mut self) {
        self.items.clear();
        self.regions.clear();
        let bounds = Rect::from_origin_size(Point::ZERO, self.size);
        // Keep the top inset, but let filling layouts reach the window bottom.
        let available_height = (self.size.height - 16.0).max(0.0);
        let height = if matches!(self.root, Widget::Column(_) | Widget::Scroll { .. }) {
            available_height
        } else {
            self.root.height().min(available_height)
        };
        let rect = Rect::new(
            16.0,
            16.0,
            (self.size.width - 16.0).max(16.0),
            16.0 + height,
        );
        layout(
            &self.root,
            rect,
            bounds,
            None,
            &mut self.items,
            &mut self.regions,
            &mut self.offsets,
        );
        self.offsets
            .retain(|id, _| self.regions.iter().any(|region| region.id == *id));
        if !self.focus.is_some_and(|id| self.enabled(id)) {
            self.focus = None;
        }
        if !self.captured.is_some_and(|id| self.enabled(id)) {
            self.captured = None;
        }
        if !self
            .key_pressed
            .is_some_and(|(id, _)| self.focus == Some(id) && self.enabled(id))
        {
            self.key_pressed = None;
        }
        self.hovered = self.pointer.and_then(|point| self.hit(point));
    }

    fn enabled(&self, id: WidgetId) -> bool {
        self.items.iter().any(|item| match &item.kind {
            ItemKind::Button {
                id: found,
                enabled: true,
                ..
            } => *found == id,
            ItemKind::Slider { slider, .. } => slider.id == id && slider.enabled && slider.valid(),
            _ => false,
        })
    }

    fn hit(&self, point: Point) -> Option<WidgetId> {
        let item = self.items.iter().rev().find(|item| {
            matches!(item.kind, ItemKind::Button { .. } | ItemKind::Slider { .. })
                && item.rect.contains(point)
                && item.clip.contains(point)
        })?;
        match &item.kind {
            ItemKind::Slider { slider, .. } if slider.enabled && slider.valid() => Some(slider.id),
            ItemKind::Button {
                id, enabled: true, ..
            } => Some(*id),
            _ => None,
        }
    }

    fn action(&self, id: WidgetId) -> Option<A> {
        self.items.iter().find_map(|item| match &item.kind {
            ItemKind::Button {
                id: found,
                enabled: true,
                action,
                ..
            } if *found == id => Some(action.clone()),
            _ => None,
        })
    }

    fn change_slider(&mut self, id: WidgetId, value: f32) -> Option<A> {
        let slider = self
            .items
            .iter_mut()
            .find_map(|item| match &mut item.kind {
                ItemKind::Slider { slider, .. }
                    if slider.id == id && slider.enabled && slider.valid() =>
                {
                    Some(slider)
                }
                _ => None,
            })?;
        let value = slider.quantize(value);
        if value == slider.quantize(slider.value) {
            return None;
        }
        slider.value = value;
        Some((slider.on_change)(value))
    }

    fn drag_slider(&mut self, id: WidgetId, point: Point) -> Option<A> {
        let (rect, min, max) = self.items.iter().find_map(|item| match &item.kind {
            ItemKind::Slider { slider, .. } if slider.id == id => {
                Some((inset(item.rect, 8.0), slider.min, slider.max))
            }
            _ => None,
        })?;
        if rect.width() <= 0.0 {
            return None;
        }
        let fraction = ((point.x - rect.x0) / rect.width()).clamp(0.0, 1.0) as f32;
        self.change_slider(id, min + fraction * (max - min))
    }

    pub fn input(&mut self, input: Input) -> Vec<A> {
        let mut actions = Vec::new();
        match input {
            Input::PointerMoved(point) => {
                self.pointer = Some(point);
                self.hovered = self.hit(point);
                if let Some(id) = self.captured
                    && let Some(action) = self.drag_slider(id, point)
                {
                    actions.push(action);
                }
            }
            Input::PointerLeft => {
                self.pointer = None;
                self.hovered = None;
            }
            Input::PointerPressed => {
                self.captured = self.hovered;
                self.focus = self.hovered;
                self.key_pressed = None;
                if let (Some(id), Some(point)) = (self.captured, self.pointer)
                    && let Some(action) = self.drag_slider(id, point)
                {
                    actions.push(action);
                }
            }
            Input::PointerReleased => {
                if let Some(id) = self.captured.take()
                    && self.hovered == Some(id)
                    && let Some(action) = self.action(id)
                {
                    actions.push(action);
                }
            }
            Input::Scroll(delta) => {
                if let Some(point) = self.pointer
                    && let Some(region) = self
                        .regions
                        .iter()
                        .rev()
                        .find(|region| region.clip.contains(point))
                {
                    let offset = self.offsets.entry(region.id).or_default();
                    *offset = (*offset + delta).clamp(0.0, region.max_offset);
                    // Geometry changes invalidate an in-progress pointer gesture.
                    self.captured = None;
                    self.layout();
                }
            }
            Input::Tab { backwards } => {
                self.key_pressed = None;
                self.captured = None;
                let ids: Vec<_> = self
                    .items
                    .iter()
                    .filter_map(|item| match item.kind {
                        ItemKind::Button {
                            id, enabled: true, ..
                        } => Some(id),
                        ItemKind::Slider { ref slider, .. } if slider.enabled && slider.valid() => {
                            Some(slider.id)
                        }
                        _ => None,
                    })
                    .collect();
                if !ids.is_empty() {
                    let index = self
                        .focus
                        .and_then(|id| ids.iter().position(|candidate| *candidate == id));
                    let next = match (index, backwards) {
                        (Some(i), true) => (i + ids.len() - 1) % ids.len(),
                        (Some(i), false) => (i + 1) % ids.len(),
                        (None, true) => ids.len() - 1,
                        (None, false) => 0,
                    };
                    self.focus = Some(ids[next]);
                    self.reveal_focus();
                }
            }
            Input::Adjust(key) => {
                if let Some(id) = self.focus {
                    let value = self.items.iter().find_map(|item| match &item.kind {
                        ItemKind::Slider { slider, .. } if slider.id == id => Some(match key {
                            AdjustmentKey::Decrease => slider.value - slider.step,
                            AdjustmentKey::Increase => slider.value + slider.step,
                            AdjustmentKey::Home => slider.min,
                            AdjustmentKey::End => slider.max,
                        }),
                        _ => None,
                    });
                    if let Some(value) = value
                        && let Some(action) = self.change_slider(id, value)
                    {
                        actions.push(action);
                    }
                }
            }
            Input::Key {
                key,
                pressed,
                repeat,
            } => {
                if pressed && !repeat && self.key_pressed.is_none() {
                    self.key_pressed = self
                        .focus
                        .filter(|id| self.enabled(*id))
                        .map(|id| (id, key));
                } else if !pressed && self.key_pressed.is_some_and(|(_, held)| held == key) {
                    let (id, _) = self.key_pressed.take().unwrap();
                    if self.focus == Some(id)
                        && let Some(action) = self.action(id)
                    {
                        actions.push(action);
                    }
                }
            }
            Input::Cancel => {
                self.pointer = None;
                self.hovered = None;
                self.captured = None;
                self.focus = None;
                self.key_pressed = None;
            }
        }
        actions
    }

    fn reveal_focus(&mut self) {
        let Some(item) = self.items.iter().find(|item| match &item.kind {
            ItemKind::Button { id, .. } => Some(*id) == self.focus,
            ItemKind::Slider { slider, .. } => Some(slider.id) == self.focus,
            _ => false,
        }) else {
            return;
        };
        let Some(region) = self
            .regions
            .iter()
            .find(|region| Some(region.id) == item.scroll)
        else {
            return;
        };
        let delta = if item.rect.y0 < region.rect.y0 {
            item.rect.y0 - region.rect.y0
        } else if item.rect.y1 > region.rect.y1 {
            item.rect.y1 - region.rect.y1
        } else {
            0.0
        };
        let offset = self.offsets.entry(region.id).or_default();
        *offset = (*offset + delta).clamp(0.0, region.max_offset);
        self.layout();
    }

    pub fn render(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        style: &WidgetStyle,
        scale: f64,
    ) {
        let transform = Affine::scale(scale);
        for item in &self.items {
            if item.rect.intersect(item.clip).area() <= 0.0 {
                continue;
            }
            scene.push_clip_layer(Fill::NonZero, transform, &item.clip);
            match &item.kind {
                ItemKind::Slider {
                    slider,
                    meter,
                    readout,
                } => {
                    if !slider.valid() {
                        scene.pop_layer();
                        continue;
                    }
                    let rail_rect = slider_rail(item.rect, meter.is_some());
                    let inner = inset(rail_rect, 8.0);
                    let fraction = f64::from(
                        (slider.quantize(slider.value) - slider.min) / (slider.max - slider.min),
                    );
                    let y = inner.center().y;
                    let rail = Rect::new(inner.x0, y - 2.0, inner.x1, y + 2.0);
                    if let Some(meter) = meter {
                        meter.render_with_range(
                            scene,
                            inner,
                            scale,
                            &style.meter,
                            f64::from(slider.min),
                            f64::from(slider.max),
                        );
                    } else {
                        scene.fill(Fill::NonZero, transform, style.meter.trough, None, &rail);
                    }
                    let x = inner.x0 + fraction * inner.width();
                    let color = if !slider.enabled {
                        style.disabled
                    } else if self.captured == Some(slider.id) {
                        style.pressed
                    } else if self.hovered == Some(slider.id) {
                        style.hovered
                    } else {
                        style.button
                    };
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        color,
                        None,
                        &Rect::new(x - 6.0, y - 10.0, x + 6.0, y + 10.0).to_rounded_rect(3.0),
                    );
                    if meter.is_some() {
                        scene.stroke(
                            &Stroke::new(1.0),
                            transform,
                            if slider.enabled {
                                style.text
                            } else {
                                style.muted
                            },
                            None,
                            &Rect::new(x - 6.0, y - 10.0, x + 6.0, y + 10.0).to_rounded_rect(3.0),
                        );
                    }
                    if let Some(readout) = readout {
                        scene.push_clip_layer(Fill::NonZero, transform, &item.rect);
                        text.draw(
                            scene,
                            readout,
                            Point::new(item.rect.x0 + 8.0, rail_rect.y1),
                            scale,
                            if slider.enabled {
                                style.text
                            } else {
                                style.muted
                            },
                        );
                        scene.pop_layer();
                    }
                    if self.focus == Some(slider.id) {
                        scene.stroke(
                            &Stroke::new(2.0),
                            transform,
                            style.focus,
                            None,
                            &inset(item.rect, 2.0).to_rounded_rect(4.0),
                        );
                    }
                }
                ItemKind::Surface => scene.fill(
                    Fill::NonZero,
                    transform,
                    style.surface,
                    None,
                    &item.rect.to_rounded_rect(6.0),
                ),
                ItemKind::StatusSurface => {
                    scene.fill(Fill::NonZero, transform, style.surface, None, &item.rect)
                }
                ItemKind::PercentageMeter(percentage) => {
                    let bar = Rect::new(
                        item.rect.x0,
                        item.rect.y0 + item.rect.height() * 0.25,
                        item.rect.x1,
                        item.rect.y0 + item.rect.height() * 0.75,
                    );
                    scene.fill(Fill::NonZero, transform, style.meter.trough, None, &bar);
                    if *percentage > 0.0 {
                        let fill = Rect::new(
                            bar.x0,
                            bar.y0,
                            bar.x0 + bar.width() * percentage / 100.0,
                            bar.y1,
                        );
                        let color = if *percentage >= 95.0 {
                            style.meter.over
                        } else if *percentage >= 80.0 {
                            style.meter.warning
                        } else {
                            style.meter.normal
                        };
                        scene.fill(Fill::NonZero, transform, color, None, &fill);
                    }
                }
                ItemKind::CompactLevelMeter(meter) => {
                    meter.render(scene, item.rect, scale, &style.meter)
                }
                ItemKind::Label { text: label, tone } => {
                    let color = match tone {
                        Tone::Normal => style.text,
                        Tone::Muted => style.muted,
                        Tone::Error => style.error,
                    };
                    scene.push_clip_layer(Fill::NonZero, transform, &item.rect);
                    text.draw(
                        scene,
                        label,
                        Point::new(item.rect.x0, item.rect.center().y - 10.0),
                        scale,
                        color,
                    );
                    scene.pop_layer();
                }
                ItemKind::LevelMeter(meter) => meter.render(scene, item.rect, scale, &style.meter),
                ItemKind::Button {
                    id, label, enabled, ..
                } => {
                    let pressed = (self.captured == Some(*id) && self.hovered == Some(*id))
                        || self.key_pressed.is_some_and(|(held, _)| held == *id);
                    let color = if !enabled {
                        style.disabled
                    } else if pressed {
                        style.pressed
                    } else if self.hovered == Some(*id) {
                        style.hovered
                    } else if label.is_none() {
                        style.surface
                    } else {
                        style.button
                    };
                    let shape = item.rect.to_rounded_rect(5.0);
                    scene.fill(Fill::NonZero, transform, color, None, &shape);
                    if self.focus == Some(*id) {
                        scene.stroke(
                            &Stroke::new(2.0),
                            transform,
                            style.focus,
                            None,
                            &inset(item.rect, 2.0).to_rounded_rect(4.0),
                        );
                    }
                    if let Some(label) = label {
                        scene.push_clip_layer(Fill::NonZero, transform, &inset(item.rect, 4.0));
                        text.draw(
                            scene,
                            label,
                            Point::new(item.rect.x0 + 12.0, item.rect.center().y - 10.0),
                            scale,
                            if *enabled { style.text } else { style.muted },
                        );
                        scene.pop_layer();
                    }
                }
            }
            scene.pop_layer();
        }
        for region in &self.regions {
            if region.max_offset <= 0.0 || region.rect.height() <= 0.0 {
                continue;
            }
            let height = region.rect.height();
            let thumb_height = (height * height / (height + region.max_offset))
                .max(16.0)
                .min(height);
            let y = region.rect.y0
                + self.offsets[&region.id] / region.max_offset * (height - thumb_height);
            let thumb = Rect::new(
                region.rect.x1 - 4.0,
                y,
                region.rect.x1 - 1.0,
                y + thumb_height,
            );
            scene.push_clip_layer(Fill::NonZero, transform, &region.clip);
            scene.fill(
                Fill::NonZero,
                transform,
                style.muted,
                None,
                &thumb.to_rounded_rect(1.5),
            );
            scene.pop_layer();
        }
    }
}

fn slider_rail(rect: Rect, combined: bool) -> Rect {
    if combined {
        Rect::new(rect.x0, rect.y0, rect.x1, (rect.y0 + 36.0).min(rect.y1))
    } else {
        rect
    }
}

fn inset(rect: Rect, padding: f64) -> Rect {
    let x = padding.min(rect.width().max(0.0) / 2.0);
    let y = padding.min(rect.height().max(0.0) / 2.0);
    Rect::new(rect.x0 + x, rect.y0 + y, rect.x1 - x, rect.y1 - y)
}

fn layout<A: Clone>(
    node: &Widget<A>,
    rect: Rect,
    clip: Rect,
    scroll: Option<WidgetId>,
    items: &mut Vec<Item<A>>,
    regions: &mut Vec<ScrollRegion>,
    offsets: &mut HashMap<WidgetId, f64>,
) {
    match node {
        Widget::Slider(slider) => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::Slider {
                slider: slider.clone(),
                meter: None,
                readout: None,
            },
        }),
        Widget::MeterSlider {
            slider,
            meter,
            readout,
        } => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::Slider {
                slider: slider.clone(),
                meter: Some(*meter),
                readout: Some(readout.clone()),
            },
        }),
        Widget::Button {
            id,
            label,
            enabled,
            action,
            ..
        } => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::Button {
                id: *id,
                label: Some(label.clone()),
                enabled: *enabled,
                action: action.clone(),
            },
        }),
        Widget::Label { text, tone, .. } => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::Label {
                text: text.clone(),
                tone: *tone,
            },
        }),
        Widget::LevelMeter { meter, .. } => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::LevelMeter(*meter),
        }),
        Widget::PercentageMeter { percentage, .. } => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::PercentageMeter(*percentage),
        }),
        Widget::CompactLevelMeter { meter, .. } => items.push(Item {
            rect,
            clip,
            scroll,
            kind: ItemKind::CompactLevelMeter(*meter),
        }),
        Widget::Row { children, .. }
        | Widget::ClickableRow { children, .. }
        | Widget::StatusBar(children) => {
            let rect = if matches!(node, Widget::StatusBar(_)) {
                Rect::new(clip.x0, rect.y0, clip.x1, rect.y1)
            } else {
                rect
            };
            let background = match node {
                Widget::StatusBar(_) => Some(ItemKind::StatusSurface),
                Widget::Row { surface: true, .. } => Some(ItemKind::Surface),
                Widget::ClickableRow {
                    id,
                    enabled,
                    action,
                    ..
                } => Some(ItemKind::Button {
                    id: *id,
                    label: None,
                    enabled: *enabled,
                    action: action.clone(),
                }),
                _ => None,
            };
            if let Some(kind) = background {
                items.push(Item {
                    rect,
                    clip,
                    scroll,
                    kind,
                });
            }
            let inner = if matches!(node, Widget::StatusBar(_)) {
                Rect::new(
                    rect.x0 + 8.0_f64.min(rect.width() / 2.0),
                    rect.y0 + 2.0,
                    rect.x1 - 8.0_f64.min(rect.width() / 2.0),
                    rect.y1 - 2.0,
                )
            } else {
                inset(rect, 8.0)
            };
            let gaps = 8.0 * children.len().saturating_sub(1) as f64;
            let fixed: f64 = children.iter().filter_map(Widget::fixed_width).sum();
            let flexible = children
                .iter()
                .filter(|child| child.fixed_width().is_none())
                .count();
            let available = (inner.width() - gaps).max(0.0);
            let fixed_scale = if fixed > available {
                available / fixed
            } else {
                1.0
            };
            let flex_width = (available - fixed).max(0.0) / flexible.max(1) as f64;
            let mut x = inner.x0;
            for child in children {
                let width = child
                    .fixed_width()
                    .map_or(flex_width, |width| width * fixed_scale);
                let height = child.height().min(inner.height().max(0.0));
                let y = inner.center().y - height / 2.0;
                layout(
                    child,
                    Rect::new(x, y, x + width, y + height),
                    clip.intersect(rect),
                    scroll,
                    items,
                    regions,
                    offsets,
                );
                x += width + 8.0;
            }
        }
        Widget::Column(children) => {
            let fixed: f64 = children.iter().map(Widget::height).sum();
            let fill_count = children
                .iter()
                .filter(|child| matches!(child, Widget::Scroll { .. }))
                .count();
            let fill = (rect.height() - fixed - 8.0 * children.len().saturating_sub(1) as f64)
                .max(0.0)
                / fill_count.max(1) as f64;
            let mut y = rect.y0;
            for child in children {
                let height = if matches!(child, Widget::Scroll { .. }) {
                    fill
                } else {
                    child.height()
                };
                layout(
                    child,
                    Rect::new(rect.x0, y, rect.x1, y + height),
                    if matches!(child, Widget::StatusBar(_)) {
                        clip
                    } else {
                        clip.intersect(rect)
                    },
                    scroll,
                    items,
                    regions,
                    offsets,
                );
                y += height + 8.0;
            }
        }
        Widget::Scroll { id, child } => {
            let height = child.height().max(rect.height());
            let max_offset = (height - rect.height()).max(0.0);
            let offset = offsets.entry(*id).or_default();
            *offset = offset.clamp(0.0, max_offset);
            let child_rect = Rect::new(
                rect.x0,
                rect.y0 - *offset,
                rect.x1,
                rect.y0 - *offset + height,
            );
            let clip = clip.intersect(rect);
            regions.push(ScrollRegion {
                id: *id,
                rect,
                clip,
                max_offset,
            });
            layout(child, child_rect, clip, Some(*id), items, regions, offsets);
        }
    }
}

#[cfg(test)]
mod tests;
