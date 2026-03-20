//! Test or Example
#![allow(dead_code)]
use gpui::{
    AnyElement, BoxShadow, Div, FontWeight, Hsla, IntoElement, MouseButton, ParentElement,
    RenderOnce, SharedString, Stateful, StyleRefinement, black, div, point, prelude::*, px, rems,
    rgba, transparent_black,
};

const SPACE_1: f32 = 4.0;
const SPACE_2: f32 = 8.0;
const SPACE_3: f32 = 12.0;
const SPACE_4: f32 = 16.0;
const SPACE_5: f32 = 24.0;

const RADIUS_1: f32 = 4.0;
const RADIUS_2: f32 = 6.0;
const RADIUS_3: f32 = 8.0;

const CONTROL_SM: f32 = 26.0;
const CONTROL_MD: f32 = 32.0;
const CONTROL_LG: f32 = 38.0;

const TEXT_XS: f32 = 11.0;
const TEXT_SM: f32 = 13.0;
const TEXT_MD: f32 = 15.0;
const TEXT_LG: f32 = 18.0;

const CARD_WIDTH: f32 = 560.0;
const DETAIL_VALUE_WIDTH: f32 = 220.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Outline,
    Ghost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlSize {
    Sm,
    Md,
    Lg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    Eyebrow,
    Title,
    Body,
    Caption,
    Metric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceTone {
    Base,
    Elevated,
    Inset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerTone {
    Neutral,
    Info,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Copy)]
struct Theme;

#[derive(Clone, Copy)]
struct ToneColors {
    background: Hsla,
    border: Hsla,
    foreground: Hsla,
}

#[derive(Clone, Copy)]
struct SurfaceStyle {
    background: Hsla,
    border: Hsla,
    shadow: ShadowKind,
}

#[derive(Clone, Copy)]
struct ButtonPalette {
    background: Hsla,
    border: Hsla,
    text: Hsla,
}

#[derive(Clone, Copy)]
enum ShadowKind {
    None,
    Raised,
}

#[derive(Clone, Copy)]
struct TextStyleSpec {
    size_rems: f32,
    line_height_rems: f32,
    weight: FontWeight,
    color: Hsla,
}

impl Theme {
    fn app_background() -> Hsla {
        rgba(0x3b41_4dff).into()
    }

    fn surface_background() -> Hsla {
        rgba(0x2f34_3eff).into()
    }

    fn elevated_surface_background() -> Hsla {
        rgba(0x2f34_3eff).into()
    }

    fn inset_surface_background() -> Hsla {
        rgba(0x282c_33ff).into()
    }

    fn element_background() -> Hsla {
        rgba(0x2e34_3eff).into()
    }

    fn element_hover() -> Hsla {
        rgba(0x363c_46ff).into()
    }

    fn element_active() -> Hsla {
        rgba(0x454a_56ff).into()
    }

    fn border() -> Hsla {
        rgba(0x464b_57ff).into()
    }

    fn border_variant() -> Hsla {
        rgba(0x363c_46ff).into()
    }

    fn focused_border() -> Hsla {
        rgba(0x4767_9eff).into()
    }

    fn text() -> Hsla {
        rgba(0xdce0_e5ff).into()
    }

    fn text_muted() -> Hsla {
        rgba(0xa9af_bcff).into()
    }

    fn text_accent() -> Hsla {
        rgba(0x74ad_e8ff).into()
    }

    fn info() -> Hsla {
        rgba(0x74ad_e8ff).into()
    }

    fn success() -> Hsla {
        rgba(0xa1c1_81ff).into()
    }

    fn warning() -> Hsla {
        rgba(0xdec1_84ff).into()
    }

    fn danger() -> Hsla {
        rgba(0xd072_77ff).into()
    }

    fn info_background() -> Hsla {
        rgba(0x74ad_e81a).into()
    }

    fn success_background() -> Hsla {
        rgba(0xa1c1_811a).into()
    }

    fn warning_background() -> Hsla {
        rgba(0xdec1_841a).into()
    }

    fn danger_background() -> Hsla {
        rgba(0xd072_771a).into()
    }

    fn info_border() -> Hsla {
        rgba(0x293b_5bff).into()
    }

    fn success_border() -> Hsla {
        rgba(0x3848_2fff).into()
    }

    fn warning_border() -> Hsla {
        rgba(0x5d4c_2fff).into()
    }

    fn danger_border() -> Hsla {
        rgba(0x4c2b_2cff).into()
    }
}

fn elevated_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: black().opacity(0.24),
            offset: point(px(0.0), px(10.0)),
            blur_radius: px(28.0),
            spread_radius: px(-12.0),
        },
        BoxShadow {
            color: black().opacity(0.3),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(0.0),
        },
    ]
}

fn control_height(size: ControlSize) -> gpui::Pixels {
    match size {
        ControlSize::Sm => px(CONTROL_SM),
        ControlSize::Md => px(CONTROL_MD),
        ControlSize::Lg => px(CONTROL_LG),
    }
}

fn control_padding_x(size: ControlSize) -> gpui::Pixels {
    match size {
        ControlSize::Sm => px(SPACE_3),
        ControlSize::Md => px(SPACE_4),
        ControlSize::Lg => px(SPACE_5),
    }
}

fn control_radius(size: ControlSize) -> gpui::Pixels {
    match size {
        ControlSize::Sm => px(RADIUS_1),
        ControlSize::Md => px(RADIUS_2),
        ControlSize::Lg => px(RADIUS_3),
    }
}

fn control_text_size(size: ControlSize) -> gpui::Rems {
    match size {
        ControlSize::Sm => rems(TEXT_XS / 16.0),
        ControlSize::Md => rems(TEXT_SM / 16.0),
        ControlSize::Lg => rems(TEXT_MD / 16.0),
    }
}

fn surface_style(tone: SurfaceTone) -> SurfaceStyle {
    match tone {
        SurfaceTone::Base => SurfaceStyle {
            background: Theme::surface_background(),
            border: Theme::border_variant(),
            shadow: ShadowKind::None,
        },
        SurfaceTone::Elevated => SurfaceStyle {
            background: Theme::elevated_surface_background(),
            border: Theme::border(),
            shadow: ShadowKind::Raised,
        },
        SurfaceTone::Inset => SurfaceStyle {
            background: Theme::inset_surface_background(),
            border: Theme::border_variant(),
            shadow: ShadowKind::None,
        },
    }
}

fn banner_tone(tone: BannerTone) -> ToneColors {
    match tone {
        BannerTone::Neutral => ToneColors {
            background: Theme::inset_surface_background(),
            border: Theme::border_variant(),
            foreground: Theme::text_muted(),
        },
        BannerTone::Info => ToneColors {
            background: Theme::info_background(),
            border: Theme::info_border(),
            foreground: Theme::info(),
        },
        BannerTone::Success => ToneColors {
            background: Theme::success_background(),
            border: Theme::success_border(),
            foreground: Theme::success(),
        },
        BannerTone::Warning => ToneColors {
            background: Theme::warning_background(),
            border: Theme::warning_border(),
            foreground: Theme::warning(),
        },
        BannerTone::Danger => ToneColors {
            background: Theme::danger_background(),
            border: Theme::danger_border(),
            foreground: Theme::danger(),
        },
    }
}

fn button_palette(variant: ButtonVariant) -> ButtonPalette {
    match variant {
        ButtonVariant::Primary => ButtonPalette {
            background: Theme::text_accent(),
            border: Theme::text_accent(),
            text: Theme::inset_surface_background(),
        },
        ButtonVariant::Secondary => ButtonPalette {
            background: Theme::element_background(),
            border: Theme::border_variant(),
            text: Theme::text(),
        },
        ButtonVariant::Outline => ButtonPalette {
            background: transparent_black(),
            border: Theme::border(),
            text: Theme::text(),
        },
        ButtonVariant::Ghost => ButtonPalette {
            background: transparent_black(),
            border: transparent_black(),
            text: Theme::text_muted(),
        },
    }
}

fn hovered_button_palette(variant: ButtonVariant) -> ButtonPalette {
    match variant {
        ButtonVariant::Primary => ButtonPalette {
            background: Theme::info(),
            border: Theme::info(),
            text: Theme::inset_surface_background(),
        },
        ButtonVariant::Secondary => ButtonPalette {
            background: Theme::element_hover(),
            border: Theme::border(),
            text: Theme::text(),
        },
        ButtonVariant::Outline => ButtonPalette {
            background: Theme::element_hover(),
            border: Theme::focused_border(),
            text: Theme::text(),
        },
        ButtonVariant::Ghost => ButtonPalette {
            background: Theme::element_hover(),
            border: transparent_black(),
            text: Theme::text(),
        },
    }
}

fn active_button_palette(variant: ButtonVariant) -> ButtonPalette {
    match variant {
        ButtonVariant::Primary => ButtonPalette {
            background: Theme::focused_border(),
            border: Theme::focused_border(),
            text: Theme::text(),
        },
        ButtonVariant::Secondary => ButtonPalette {
            background: Theme::element_active(),
            border: Theme::border(),
            text: Theme::text(),
        },
        ButtonVariant::Outline => ButtonPalette {
            background: Theme::element_active(),
            border: Theme::focused_border(),
            text: Theme::text(),
        },
        ButtonVariant::Ghost => ButtonPalette {
            background: Theme::element_active(),
            border: transparent_black(),
            text: Theme::text(),
        },
    }
}

fn text_style(role: TextRole) -> TextStyleSpec {
    match role {
        TextRole::Eyebrow => TextStyleSpec {
            size_rems: TEXT_XS / 16.0,
            line_height_rems: 1.0,
            weight: FontWeight::SEMIBOLD,
            color: Theme::text_muted(),
        },
        TextRole::Title => TextStyleSpec {
            size_rems: TEXT_LG / 16.0,
            line_height_rems: 1.35,
            weight: FontWeight::SEMIBOLD,
            color: Theme::text(),
        },
        TextRole::Body => TextStyleSpec {
            size_rems: TEXT_MD / 16.0,
            line_height_rems: 1.45,
            weight: FontWeight::NORMAL,
            color: Theme::text(),
        },
        TextRole::Caption => TextStyleSpec {
            size_rems: TEXT_SM / 16.0,
            line_height_rems: 1.35,
            weight: FontWeight::NORMAL,
            color: Theme::text_muted(),
        },
        TextRole::Metric => TextStyleSpec {
            size_rems: TEXT_LG / 16.0,
            line_height_rems: 1.2,
            weight: FontWeight::BOLD,
            color: Theme::text(),
        },
    }
}

#[derive(IntoElement)]
pub struct AppFrame {
    children: Vec<AnyElement>,
}

impl AppFrame {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl Default for AppFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for AppFrame {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .px(px(SPACE_5))
            .py(px(20.0))
            .bg(Theme::app_background())
            .text_color(Theme::text())
            .children(self.children)
    }
}

#[derive(IntoElement)]
pub struct Card {
    tone: SurfaceTone,
    children: Vec<AnyElement>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            tone: SurfaceTone::Elevated,
            children: Vec::new(),
        }
    }

    pub fn tone(mut self, tone: SurfaceTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        let style = surface_style(self.tone);

        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_4))
            .w(px(CARD_WIDTH))
            .p(px(20.0))
            .rounded(px(RADIUS_3))
            .bg(style.background)
            .border_1()
            .border_color(style.border)
            .when(matches!(style.shadow, ShadowKind::Raised), |this| {
                this.shadow(elevated_shadow())
            })
            .children(self.children)
    }
}

#[derive(IntoElement)]
pub struct Text {
    role: TextRole,
    content: SharedString,
}

impl Text {
    pub fn new(role: TextRole, content: impl Into<SharedString>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

impl RenderOnce for Text {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        let style = text_style(self.role);

        div()
            .text_size(rems(style.size_rems))
            .line_height(rems(style.line_height_rems))
            .font_weight(style.weight)
            .text_color(style.color)
            .child(self.content)
    }
}

#[derive(IntoElement)]
pub struct Metric {
    content: SharedString,
}

impl Metric {
    pub fn new(content: impl Into<SharedString>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl RenderOnce for Metric {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        Text::new(TextRole::Metric, self.content).render(window, cx)
    }
}

#[derive(IntoElement)]
pub struct DetailRow {
    label: SharedString,
    value: SharedString,
}

impl DetailRow {
    pub fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

impl RenderOnce for DetailRow {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(SPACE_3))
            .px(px(SPACE_3))
            .py(px(SPACE_2))
            .rounded(px(RADIUS_2))
            .bg(Theme::inset_surface_background())
            .border_1()
            .border_color(Theme::border_variant())
            .child(
                div()
                    .flex_1()
                    .text_size(rems(TEXT_XS / 16.0))
                    .line_height(rems(1.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(Theme::text_muted())
                    .child(self.label),
            )
            .child(
                div()
                    .w(px(DETAIL_VALUE_WIDTH))
                    .text_right()
                    .text_size(rems(TEXT_SM / 16.0))
                    .line_height(rems(1.15))
                    .text_color(Theme::text())
                    .child(self.value),
            )
    }
}

#[derive(IntoElement)]
pub struct Banner {
    tone: BannerTone,
    content: SharedString,
}

impl Banner {
    pub fn new(content: impl Into<SharedString>) -> Self {
        Self {
            tone: BannerTone::Neutral,
            content: content.into(),
        }
    }

    pub fn tone(mut self, tone: BannerTone) -> Self {
        self.tone = tone;
        self
    }
}

impl RenderOnce for Banner {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        let tone = banner_tone(self.tone);

        div()
            .px(px(SPACE_3))
            .py(px(SPACE_2))
            .rounded(px(RADIUS_2))
            .bg(tone.background)
            .border_1()
            .border_color(tone.border)
            .child(
                div()
                    .text_size(rems(TEXT_SM / 16.0))
                    .line_height(rems(1.3))
                    .text_color(tone.foreground)
                    .child(self.content),
            )
    }
}

#[derive(IntoElement)]
pub struct Badge {
    tone: BannerTone,
    content: SharedString,
}

impl Badge {
    pub fn new(content: impl Into<SharedString>) -> Self {
        Self {
            tone: BannerTone::Neutral,
            content: content.into(),
        }
    }

    pub fn tone(mut self, tone: BannerTone) -> Self {
        self.tone = tone;
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        let tone = banner_tone(self.tone);

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .rounded(px(RADIUS_2))
            .bg(tone.background)
            .border_1()
            .border_color(tone.border)
            .child(
                div()
                    .text_size(rems(TEXT_XS / 16.0))
                    .line_height(rems(1.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(tone.foreground)
                    .whitespace_nowrap()
                    .child(self.content),
            )
    }
}

pub struct Button {
    id: SharedString,
    label: SharedString,
    variant: ButtonVariant,
    size: ControlSize,
}

impl Button {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::Secondary,
            size: ControlSize::Md,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn build(self) -> Stateful<Div> {
        let enabled = button_palette(self.variant);
        let hovered = hovered_button_palette(self.variant);
        let active = active_button_palette(self.variant);

        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(control_height(self.size))
            .px(control_padding_x(self.size))
            .rounded(control_radius(self.size))
            .bg(enabled.background)
            .border_1()
            .border_color(enabled.border)
            .text_color(enabled.text)
            .cursor_pointer()
            .focusable()
            .focus(|style| style.border_color(Theme::focused_border()))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .hover(move |style: StyleRefinement| {
                style
                    .bg(hovered.background)
                    .border_color(hovered.border)
                    .text_color(hovered.text)
            })
            .active(move |style: StyleRefinement| {
                style
                    .bg(active.background)
                    .border_color(active.border)
                    .text_color(active.text)
            })
            .child(
                div()
                    .text_size(control_text_size(self.size))
                    .line_height(rems(1.0))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .child(self.label),
            )
    }
}

pub fn action_row() -> Div {
    div().flex().gap(px(SPACE_2))
}
