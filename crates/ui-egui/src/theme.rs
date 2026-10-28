//! Design tokens (light and dark), the graph's lane colours and the egui style.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

/// GitCraft's app colour (and its darker ink for text on light backgrounds).
pub const APP_COLOR: Color32 = Color32::from_rgb(0x70, 0x48, 0xE8);
pub const APP_INK: Color32 = Color32::from_rgb(0x59, 0x37, 0xC2);

/// The interface theme: fixed light or dark, or following the system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];
    pub fn code(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "Use system setting",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
    pub fn from_code(s: &str) -> Option<Appearance> {
        Appearance::ALL.into_iter().find(|a| a.code().eq_ignore_ascii_case(s.trim()))
    }
    pub fn preference(self) -> egui::ThemePreference {
        match self {
            Appearance::System => egui::ThemePreference::System,
            Appearance::Light => egui::ThemePreference::Light,
            Appearance::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// Colours the views paint with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// The window behind everything.
    pub bg: Color32,
    /// Side panels.
    pub panel: Color32,
    /// Section headers, the toolbar.
    pub chrome: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub hover: Color32,
    pub added: Color32,
    pub added_bg: Color32,
    pub removed: Color32,
    pub removed_bg: Color32,
    pub modified: Color32,
    pub warning: Color32,
    pub warning_bg: Color32,
    pub danger: Color32,
    pub hunk_bg: Color32,
    pub picked_bg: Color32,
    /// The words that changed within a removed or added line.
    pub removed_word_bg: Color32,
    pub added_word_bg: Color32,
    pub syn_keyword: Color32,
    pub syn_type: Color32,
    pub syn_string: Color32,
    pub syn_comment: Color32,
    pub syn_number: Color32,
}

impl Tokens {
    pub const LIGHT: Tokens = Tokens {
        dark: false,
        bg: Color32::from_rgb(0xFA, 0xFA, 0xFC),
        panel: Color32::from_rgb(0xF1, 0xF2, 0xF6),
        chrome: Color32::from_rgb(0xE7, 0xE8, 0xEE),
        border: Color32::from_rgb(0xD5, 0xD7, 0xE0),
        text: Color32::from_rgb(0x1F, 0x22, 0x2B),
        dim: Color32::from_rgb(0x6A, 0x70, 0x80),
        accent: APP_INK,
        selection: Color32::from_rgb(0xE2, 0xDA, 0xFB),
        hover: Color32::from_rgb(0xEC, 0xE9, 0xF7),
        added: Color32::from_rgb(0x1E, 0x8E, 0x3E),
        added_bg: Color32::from_rgb(0xE3, 0xF6, 0xE7),
        removed: Color32::from_rgb(0xC9, 0x2A, 0x2A),
        removed_bg: Color32::from_rgb(0xFC, 0xE6, 0xE6),
        modified: Color32::from_rgb(0xB7, 0x79, 0x1F),
        warning: Color32::from_rgb(0x8A, 0x5A, 0x00),
        warning_bg: Color32::from_rgb(0xFF, 0xF1, 0xCC),
        danger: Color32::from_rgb(0xC9, 0x2A, 0x2A),
        hunk_bg: Color32::from_rgb(0xEC, 0xEE, 0xF5),
        picked_bg: Color32::from_rgb(0xD6, 0xCC, 0xFA),
        removed_word_bg: Color32::from_rgb(0xF5, 0xB8, 0xB8),
        added_word_bg: Color32::from_rgb(0xAC, 0xE3, 0xB6),
        syn_keyword: Color32::from_rgb(0x8B, 0x2F, 0xC9),
        syn_type: Color32::from_rgb(0x0B, 0x6E, 0x99),
        syn_string: Color32::from_rgb(0x2B, 0x7A, 0x0B),
        syn_comment: Color32::from_rgb(0x7A, 0x80, 0x8E),
        syn_number: Color32::from_rgb(0xB3, 0x5C, 0x00),
    };
    pub const DARK: Tokens = Tokens {
        dark: true,
        bg: Color32::from_rgb(0x1A, 0x1B, 0x21),
        panel: Color32::from_rgb(0x22, 0x23, 0x2B),
        chrome: Color32::from_rgb(0x2A, 0x2B, 0x35),
        border: Color32::from_rgb(0x3A, 0x3C, 0x48),
        text: Color32::from_rgb(0xE4, 0xE5, 0xEC),
        dim: Color32::from_rgb(0x95, 0x99, 0xAA),
        accent: Color32::from_rgb(0xA4, 0x8C, 0xF5),
        selection: Color32::from_rgb(0x3D, 0x33, 0x66),
        hover: Color32::from_rgb(0x2E, 0x2D, 0x3E),
        added: Color32::from_rgb(0x5C, 0xCB, 0x75),
        added_bg: Color32::from_rgb(0x1D, 0x33, 0x24),
        removed: Color32::from_rgb(0xF0, 0x71, 0x71),
        removed_bg: Color32::from_rgb(0x3D, 0x20, 0x22),
        modified: Color32::from_rgb(0xE8, 0xB0, 0x4F),
        warning: Color32::from_rgb(0xF5, 0xC2, 0x5A),
        warning_bg: Color32::from_rgb(0x3D, 0x32, 0x16),
        danger: Color32::from_rgb(0xF0, 0x71, 0x71),
        hunk_bg: Color32::from_rgb(0x26, 0x28, 0x34),
        picked_bg: Color32::from_rgb(0x4A, 0x3D, 0x80),
        removed_word_bg: Color32::from_rgb(0x6B, 0x2C, 0x30),
        added_word_bg: Color32::from_rgb(0x2B, 0x5C, 0x36),
        syn_keyword: Color32::from_rgb(0xC7, 0x92, 0xEA),
        syn_type: Color32::from_rgb(0x6C, 0xC4, 0xE8),
        syn_string: Color32::from_rgb(0xA5, 0xD6, 0x7F),
        syn_comment: Color32::from_rgb(0x82, 0x88, 0x99),
        syn_number: Color32::from_rgb(0xF2, 0xA4, 0x6B),
    };

    pub fn get(ctx: &egui::Context) -> Tokens {
        if ctx.global_style().visuals.dark_mode { Tokens::DARK } else { Tokens::LIGHT }
    }
}

/// The graph's lane colours: original, chosen to stay apart from each other in both themes.
const LANES: [Color32; 10] = [
    Color32::from_rgb(0x70, 0x48, 0xE8),
    Color32::from_rgb(0x12, 0xA5, 0x94),
    Color32::from_rgb(0xF0, 0x8C, 0x00),
    Color32::from_rgb(0xD6, 0x33, 0x84),
    Color32::from_rgb(0x2F, 0x80, 0xED),
    Color32::from_rgb(0x40, 0xA0, 0x2B),
    Color32::from_rgb(0xE0, 0x4E, 0x39),
    Color32::from_rgb(0x8E, 0x5B, 0x2E),
    Color32::from_rgb(0x15, 0xAA, 0xBF),
    Color32::from_rgb(0xAE, 0x3E, 0xC9),
];

pub fn lane_color(i: usize) -> Color32 {
    LANES[i % LANES.len()]
}

/// Text on a lane colour.
pub fn on_lane() -> Color32 {
    Color32::WHITE
}

/// The egui style for a theme.
pub fn style(ctx: &egui::Context) {
    for (theme, t) in [(egui::Theme::Light, Tokens::LIGHT), (egui::Theme::Dark, Tokens::DARK)] {
        ctx.style_mut_of(theme, |s| {
            let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
            v.panel_fill = t.bg;
            v.window_fill = t.panel;
            v.extreme_bg_color = if t.dark { Color32::from_rgb(0x15, 0x16, 0x1B) } else { Color32::WHITE };
            v.faint_bg_color = t.panel;
            v.selection.bg_fill = t.selection;
            v.selection.stroke = Stroke::new(1.0, t.accent);
            v.hyperlink_color = t.accent;
            v.window_corner_radius = CornerRadius::same(8);
            v.menu_corner_radius = CornerRadius::same(6);
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
            v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
            v.widgets.inactive.corner_radius = CornerRadius::same(4);
            v.widgets.hovered.corner_radius = CornerRadius::same(4);
            v.widgets.active.corner_radius = CornerRadius::same(4);
            s.visuals = v;
            s.spacing.item_spacing = egui::vec2(6.0, 4.0);
            s.spacing.button_padding = egui::vec2(8.0, 3.0);
            s.text_styles.insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace));
            s.text_styles.insert(TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional));
        });
    }
}
