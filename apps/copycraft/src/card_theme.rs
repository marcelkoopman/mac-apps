//! Card themes (Settings › Display › Card theme): the colours the card's well is painted in.
//!
//! [`CardTheme::System`] has no palette of its own: the card keeps the dynamic system colours
//! (label, secondary label, system blue/green/…) on the see-through glass well, so it follows
//! the macOS appearance live without any observer. Every other theme is a fixed sRGB
//! [`Palette`] on an opaque well, the same in light and dark mode.
//!
//! Pure, so the contrast rules are tested on every platform. What the tests check (WCAG 2
//! relative luminance, [`mac_ui::style::contrast`]), against the theme background:
//! - text, secondary text, accent, link, every code token colour (key, string, number,
//!   keyword, type, function, comment) and the diff add/remove colours: at least 4.5:1
//!   (AA body text; the code font is 12 pt);
//! - tertiary text (punctuation, line numbers) and the dimmed link path: at least 3:1
//!   (AA large text / non-text; they never carry meaning alone);
//! - the diff add/remove colours on their own row background, and the text on the selection,
//!   the diff row backgrounds: at least 4.5:1;
//! - High contrast: every one of the above at least 7:1 (AAA).

/// A colour in sRGB, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// The channels from 0 to 1, for AppKit and for the contrast formula.
    pub fn unit(self) -> (f64, f64, f64) {
        (
            f64::from(self.0) / 255.0,
            f64::from(self.1) / 255.0,
            f64::from(self.2) / 255.0,
        )
    }
}

impl Rgb {
    /// This colour moved `amount` (0 to 1) of the way toward `other`, per channel.
    pub fn mix(self, other: Rgb, amount: f64) -> Rgb {
        let channel = |from: u8, to: u8| {
            let value = f64::from(from) + (f64::from(to) - f64::from(from)) * amount;
            value.round().clamp(0.0, 255.0) as u8
        };
        Rgb(
            channel(self.0, other.0),
            channel(self.1, other.1),
            channel(self.2, other.2),
        )
    }
}

/// The colours of a history strip tile in a fixed theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileColours {
    pub background: Rgb,
    pub text: Rgb,
    /// The type mark above the lines.
    pub mark: Rgb,
}

impl Palette {
    /// A history strip tile: the theme's background with its text and secondary text; the
    /// tile of the copy on screen on the selection colour, its mark in the text colour.
    pub fn strip_tile(&self, current: bool) -> TileColours {
        if current {
            TileColours {
                background: self.selection,
                text: self.text,
                mark: self.text,
            }
        } else {
            TileColours {
                background: self.background,
                text: self.text,
                mark: self.secondary,
            }
        }
    }

    /// The odd rows of the table window's grid: the background moved 6 % toward the text.
    pub fn table_stripe(&self) -> Rgb {
        self.background.mix(self.text, 0.06)
    }
}

/// WCAG contrast ratio of two colours (1 to 21).
#[cfg_attr(not(test), allow(dead_code))]
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    mac_ui::style::contrast(a.unit(), b.unit())
}

/// Every colour a fixed theme paints the card with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The well's opaque fill.
    pub background: Rgb,
    /// Body text and plain code.
    pub text: Rgb,
    /// Captions, link schemes, the diff's unchanged rows.
    pub secondary: Rgb,
    /// Punctuation and line numbers.
    pub tertiary: Rgb,
    /// CSV/TSV headers and the Settings preview's accent.
    pub accent: Rgb,
    /// A link's host.
    pub link: Rgb,
    /// A link's path, query and fragment.
    pub link_dim: Rgb,
    /// Selected text background (Settings preview; see the README for the card).
    pub selection: Rgb,
    pub key: Rgb,
    pub string: Rgb,
    pub number: Rgb,
    pub keyword: Rgb,
    pub type_name: Rgb,
    /// Functions and macros.
    pub function: Rgb,
    pub comment: Rgb,
    /// Diff: an added side, and its row background.
    pub diff_add: Rgb,
    pub diff_add_bg: Rgb,
    /// Diff: a removed side, and its row background.
    pub diff_remove: Rgb,
    pub diff_remove_bg: Rgb,
    /// Dark background: the card window takes the dark appearance (controls, glass, the
    /// system selection), so the chrome around the well matches the theme.
    pub dark: bool,
}

/// The card themes, in the order of the Settings popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CardTheme {
    /// Follow macOS (dynamic system colours, glass well).
    #[default]
    System,
    Light,
    Dark,
    /// The icon's navy.
    Night,
    SolarizedLight,
    SolarizedDark,
    HighContrast,
    Sepia,
    Nord,
    GruvboxDark,
    Dracula,
    Monokai,
    CatppuccinMocha,
    TokyoNight,
    /// CopyCraft's own vivid blue and teal.
    Ocean,
    /// CopyCraft's own vivid green.
    Forest,
}

impl CardTheme {
    /// In popup order: the basic themes, the classic ones, then the colourful ones
    /// ([`CardTheme::group`]; the popup puts a separator between groups).
    pub const ALL: [CardTheme; 16] = [
        CardTheme::System,
        CardTheme::Light,
        CardTheme::Dark,
        CardTheme::HighContrast,
        CardTheme::Night,
        CardTheme::Sepia,
        CardTheme::SolarizedLight,
        CardTheme::SolarizedDark,
        CardTheme::Nord,
        CardTheme::GruvboxDark,
        CardTheme::Dracula,
        CardTheme::Monokai,
        CardTheme::CatppuccinMocha,
        CardTheme::TokyoNight,
        CardTheme::Ocean,
        CardTheme::Forest,
    ];

    /// The theme's name, NL/EN (Settings › Display and `⋯` › Theme ▸).
    pub fn title(self) -> &'static str {
        match self {
            CardTheme::System => crate::locale::t("card_theme_system"),
            CardTheme::Light => crate::locale::t("card_theme_light"),
            CardTheme::Dark => crate::locale::t("card_theme_dark"),
            CardTheme::Night => crate::locale::t("card_theme_night"),
            CardTheme::SolarizedLight => crate::locale::t("card_theme_solarized_light"),
            CardTheme::SolarizedDark => crate::locale::t("card_theme_solarized_dark"),
            CardTheme::HighContrast => crate::locale::t("card_theme_high_contrast"),
            CardTheme::Sepia => crate::locale::t("card_theme_sepia"),
            CardTheme::Nord => crate::locale::t("card_theme_nord"),
            CardTheme::GruvboxDark => crate::locale::t("card_theme_gruvbox_dark"),
            CardTheme::Dracula => crate::locale::t("card_theme_dracula"),
            CardTheme::Monokai => crate::locale::t("card_theme_monokai"),
            CardTheme::CatppuccinMocha => crate::locale::t("card_theme_catppuccin_mocha"),
            CardTheme::TokyoNight => crate::locale::t("card_theme_tokyo_night"),
            CardTheme::Ocean => crate::locale::t("card_theme_ocean"),
            CardTheme::Forest => crate::locale::t("card_theme_forest"),
        }
    }

    /// The popup group: 0 basic, 1 classic, 2 colourful.
    pub fn group(self) -> usize {
        match self {
            CardTheme::System | CardTheme::Light | CardTheme::Dark | CardTheme::HighContrast => 0,
            CardTheme::Night
            | CardTheme::Sepia
            | CardTheme::SolarizedLight
            | CardTheme::SolarizedDark
            | CardTheme::Nord
            | CardTheme::GruvboxDark => 1,
            CardTheme::Dracula
            | CardTheme::Monokai
            | CardTheme::CatppuccinMocha
            | CardTheme::TokyoNight
            | CardTheme::Ocean
            | CardTheme::Forest => 2,
        }
    }

    /// Value stored in the user defaults.
    pub fn as_str(self) -> &'static str {
        match self {
            CardTheme::System => "system",
            CardTheme::Light => "light",
            CardTheme::Dark => "dark",
            CardTheme::Night => "night",
            CardTheme::SolarizedLight => "solarized-light",
            CardTheme::SolarizedDark => "solarized-dark",
            CardTheme::HighContrast => "high-contrast",
            CardTheme::Sepia => "sepia",
            CardTheme::Nord => "nord",
            CardTheme::GruvboxDark => "gruvbox-dark",
            CardTheme::Dracula => "dracula",
            CardTheme::Monokai => "monokai",
            CardTheme::CatppuccinMocha => "catppuccin-mocha",
            CardTheme::TokyoNight => "tokyo-night",
            CardTheme::Ocean => "ocean",
            CardTheme::Forest => "forest",
        }
    }

    /// Inverse of [`CardTheme::as_str`]. Unknown values are `None` (callers fall back to System).
    pub fn parse(value: &str) -> Option<Self> {
        CardTheme::ALL
            .into_iter()
            .find(|theme| theme.as_str() == value)
    }

    /// Position in the Settings popup ([`CardTheme::ALL`]).
    pub fn index(self) -> usize {
        CardTheme::ALL
            .iter()
            .position(|theme| *theme == self)
            .unwrap_or(0)
    }

    /// The theme at popup position `index`; out of range is System.
    pub fn at(index: usize) -> Self {
        CardTheme::ALL.get(index).copied().unwrap_or_default()
    }

    /// Minimum text/background contrast the theme promises (checked by the tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn min_contrast(self) -> f64 {
        match self {
            CardTheme::HighContrast => 7.0,
            _ => mac_ui::style::AA_BODY,
        }
    }

    /// The fixed palette; `None` for System (dynamic system colours).
    pub fn palette(self) -> Option<Palette> {
        match self {
            CardTheme::System => None,
            CardTheme::Light => Some(LIGHT),
            CardTheme::Dark => Some(DARK),
            CardTheme::Night => Some(NIGHT),
            CardTheme::SolarizedLight => Some(SOLARIZED_LIGHT),
            CardTheme::SolarizedDark => Some(SOLARIZED_DARK),
            CardTheme::HighContrast => Some(HIGH_CONTRAST),
            CardTheme::Sepia => Some(SEPIA),
            CardTheme::Nord => Some(NORD),
            CardTheme::GruvboxDark => Some(GRUVBOX_DARK),
            CardTheme::Dracula => Some(DRACULA),
            CardTheme::Monokai => Some(MONOKAI),
            CardTheme::CatppuccinMocha => Some(CATPPUCCIN_MOCHA),
            CardTheme::TokyoNight => Some(TOKYO_NIGHT),
            CardTheme::Ocean => Some(OCEAN),
            CardTheme::Forest => Some(FOREST),
        }
    }
}

const fn hex(value: u32) -> Rgb {
    Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// Close to macOS light mode on white.
const LIGHT: Palette = Palette {
    background: hex(0xFFFFFF),
    text: hex(0x1D1D1F),
    secondary: hex(0x5E5E63),
    tertiary: hex(0x8A8A8E),
    accent: hex(0x0A60C2),
    link: hex(0x0A5CC2),
    link_dim: hex(0x5A7FB5),
    selection: hex(0xB3D7FF),
    key: hex(0x0B57B5),
    string: hex(0x1A7F37),
    number: hex(0xA04B00),
    keyword: hex(0x8A2BB0),
    type_name: hex(0x0E7490),
    function: hex(0x0B57B5),
    comment: hex(0x6B6B70),
    diff_add: hex(0x116329),
    diff_add_bg: hex(0xDAFBE1),
    diff_remove: hex(0xA40E26),
    diff_remove_bg: hex(0xFFEBE9),
    dark: false,
};

/// Close to macOS dark mode, on a solid dark grey.
const DARK: Palette = Palette {
    background: hex(0x1E1E1E),
    text: hex(0xEDEDED),
    secondary: hex(0xA8A8AD),
    tertiary: hex(0x7C7C80),
    accent: hex(0x4DA3FF),
    link: hex(0x5AAEFF),
    link_dim: hex(0x4A88C7),
    selection: hex(0x3F638B),
    key: hex(0x6CB6FF),
    string: hex(0x7EE787),
    number: hex(0xFFA657),
    keyword: hex(0xD2A8FF),
    type_name: hex(0x56D4DD),
    function: hex(0x79C0FF),
    comment: hex(0x9A9AA0),
    diff_add: hex(0x7EE787),
    diff_add_bg: hex(0x12341D),
    diff_remove: hex(0xFFA198),
    diff_remove_bg: hex(0x3D1518),
    dark: true,
};

/// The navy of the app icon ([`mac_ui::style::ACCENT_RGB`], #141E3C).
const NIGHT: Palette = Palette {
    background: hex(0x141E3C),
    text: hex(0xF2F5FA),
    secondary: hex(0xAEB8CC),
    tertiary: hex(0x7D89A3),
    accent: hex(0x8CB8FF),
    link: hex(0x8CC4FF),
    link_dim: hex(0x6A93C4),
    selection: hex(0x2E4777),
    key: hex(0x8CC4FF),
    string: hex(0xA5E3A0),
    number: hex(0xFFC27A),
    keyword: hex(0xD7B4FF),
    type_name: hex(0x7FDCE0),
    function: hex(0x9CD0FF),
    comment: hex(0x9AA6BD),
    diff_add: hex(0xA5E3A0),
    diff_add_bg: hex(0x1D3A35),
    diff_remove: hex(0xFFAAA5),
    diff_remove_bg: hex(0x40243A),
    dark: true,
};

/// Solarized (Ethan Schoonover) base3 background. Body text is base02 and secondary base01;
/// the accent hues are darkened where the originals stay under 4.5:1 on base3.
const SOLARIZED_LIGHT: Palette = Palette {
    background: hex(0xFDF6E3),
    text: hex(0x073642),
    secondary: hex(0x586E75),
    tertiary: hex(0x7B8C8E),
    accent: hex(0x1D6FA8),
    link: hex(0x1D6FA8),
    link_dim: hex(0x4F86AD),
    selection: hex(0xEEE8D5),
    key: hex(0x1D6FA8),
    string: hex(0x1E7A72),
    number: hex(0xA63B0F),
    keyword: hex(0x5E6B00),
    type_name: hex(0x7E5F00),
    function: hex(0x2A6597),
    comment: hex(0x586E75),
    diff_add: hex(0x5E6B00),
    diff_add_bg: hex(0xEEF0D0),
    diff_remove: hex(0xB0231F),
    diff_remove_bg: hex(0xF8E1D8),
    dark: false,
};

/// Solarized base03 background. Body text is base1 and secondary base0; the accent hues are
/// lightened where the originals stay under 4.5:1 on base03.
const SOLARIZED_DARK: Palette = Palette {
    background: hex(0x002B36),
    text: hex(0x93A1A1),
    secondary: hex(0x839496),
    tertiary: hex(0x657B83),
    accent: hex(0x3B9CE0),
    link: hex(0x3B9CE0),
    link_dim: hex(0x2F7FB6),
    selection: hex(0x073642),
    key: hex(0x3B9CE0),
    string: hex(0x2AA198),
    number: hex(0xE0702F),
    keyword: hex(0x95AB00),
    type_name: hex(0xC99A00),
    function: hex(0x5BAAE6),
    comment: hex(0x839496),
    diff_add: hex(0x95AB00),
    diff_add_bg: hex(0x0E3A30),
    diff_remove: hex(0xF0605A),
    diff_remove_bg: hex(0x3A1F25),
    dark: true,
};

/// White on black; every colour at least 7:1.
const HIGH_CONTRAST: Palette = Palette {
    background: hex(0x000000),
    text: hex(0xFFFFFF),
    secondary: hex(0xE6E6E6),
    tertiary: hex(0xBDBDBD),
    accent: hex(0xFFD60A),
    link: hex(0x7CC8FF),
    link_dim: hex(0x6AB4F0),
    selection: hex(0x1F4E99),
    key: hex(0x7CC8FF),
    string: hex(0x8DF59A),
    number: hex(0xFFC266),
    keyword: hex(0xE6B3FF),
    type_name: hex(0x66F0F0),
    function: hex(0xA6D9FF),
    comment: hex(0xD9D9D9),
    diff_add: hex(0x8DF59A),
    diff_add_bg: hex(0x0A2E12),
    diff_remove: hex(0xFFA3A3),
    diff_remove_bg: hex(0x3A0C0C),
    dark: true,
};

/// Warm paper with brown ink.
const SEPIA: Palette = Palette {
    background: hex(0xF4ECD8),
    text: hex(0x3B2F20),
    secondary: hex(0x6B5A43),
    tertiary: hex(0x8C7A60),
    accent: hex(0x8A4B12),
    link: hex(0x7A3E0E),
    link_dim: hex(0x9A6A45),
    selection: hex(0xE3D3B0),
    key: hex(0x7A3E0E),
    string: hex(0x4F6B1E),
    number: hex(0x934700),
    keyword: hex(0x7B2D6B),
    type_name: hex(0x2F6470),
    function: hex(0x6B4310),
    comment: hex(0x6E6150),
    diff_add: hex(0x3F5F12),
    diff_add_bg: hex(0xE3EBC8),
    diff_remove: hex(0x9B2A1A),
    diff_remove_bg: hex(0xF2D6C8),
    dark: false,
};

/// Dracula (draculatheme.com): the official palette. The comment colour is lightened
/// (#6272A4 is 3.0:1 on the background) and the secondary text is a lighter tint of it.
const DRACULA: Palette = Palette {
    background: hex(0x282A36),
    text: hex(0xF8F8F2),
    secondary: hex(0xBFC1D0),
    tertiary: hex(0x8A93B8),
    accent: hex(0xBD93F9),
    link: hex(0x8BE9FD),
    link_dim: hex(0x6FB8CC),
    selection: hex(0x44475A),
    key: hex(0x8BE9FD),
    string: hex(0xF1FA8C),
    number: hex(0xFFB86C),
    keyword: hex(0xFF79C6),
    type_name: hex(0x8BE9FD),
    function: hex(0x50FA7B),
    comment: hex(0x8C9AC9),
    diff_add: hex(0x50FA7B),
    diff_add_bg: hex(0x1F3B2B),
    diff_remove: hex(0xFF5555),
    diff_remove_bg: hex(0x3D2230),
    dark: true,
};

/// Nord (nordtheme.com): Polar Night nord0 background, Snow Storm text, Frost and Aurora
/// accents. Lightened where the originals stay under 4.5:1: orange #D08770, purple #B48EAD,
/// red #BF616A (the diff); the comment is a lighter nord3 (#4C566A is 1.7:1).
const NORD: Palette = Palette {
    background: hex(0x2E3440),
    text: hex(0xECEFF4),
    secondary: hex(0xD8DEE9),
    tertiary: hex(0x8C97AD),
    accent: hex(0x88C0D0),
    link: hex(0x88C0D0),
    link_dim: hex(0x81A1C1),
    selection: hex(0x434C5E),
    key: hex(0x81A1C1),
    string: hex(0xA3BE8C),
    number: hex(0xD8916F),
    keyword: hex(0xBD98B6),
    type_name: hex(0x8FBCBB),
    function: hex(0x88C0D0),
    comment: hex(0x9AA5BA),
    diff_add: hex(0xA3BE8C),
    diff_add_bg: hex(0x34403A),
    diff_remove: hex(0xD8919A),
    diff_remove_bg: hex(0x3B2F3A),
    dark: true,
};

/// Gruvbox Dark (morhetz): bg0, fg1 and the bright accents. The bright red #FB4934 is
/// lightened (4.3:1); comments use fg4 #A89984, since gray #928374 is 4.0:1.
const GRUVBOX_DARK: Palette = Palette {
    background: hex(0x282828),
    text: hex(0xEBDBB2),
    secondary: hex(0xBDAE93),
    tertiary: hex(0x928374),
    accent: hex(0xFABD2F),
    link: hex(0x83A598),
    link_dim: hex(0x7C9A8E),
    selection: hex(0x504945),
    key: hex(0x83A598),
    string: hex(0xB8BB26),
    number: hex(0xD3869B),
    keyword: hex(0xFB5D4A),
    type_name: hex(0xFABD2F),
    function: hex(0x8EC07C),
    comment: hex(0xA89984),
    diff_add: hex(0xB8BB26),
    diff_add_bg: hex(0x32361A),
    diff_remove: hex(0xFB5D4A),
    diff_remove_bg: hex(0x351C1B),
    dark: true,
};

/// Monokai (Wimer Hazenberg): the classic palette. The pink #F92672 is lightened (3.9:1) and
/// the comment #75715E too (3.0:1).
const MONOKAI: Palette = Palette {
    background: hex(0x272822),
    text: hex(0xF8F8F2),
    secondary: hex(0xCFCFC2),
    tertiary: hex(0x90908A),
    accent: hex(0xA6E22E),
    link: hex(0x66D9EF),
    link_dim: hex(0x5AB0C2),
    selection: hex(0x49483E),
    key: hex(0x66D9EF),
    string: hex(0xE6DB74),
    number: hex(0xAE81FF),
    keyword: hex(0xFF4F8B),
    type_name: hex(0x66D9EF),
    function: hex(0xA6E22E),
    comment: hex(0x9D9883),
    diff_add: hex(0xA6E22E),
    diff_add_bg: hex(0x2F3A1C),
    diff_remove: hex(0xFF4F8B),
    diff_remove_bg: hex(0x361B24),
    dark: true,
};

/// Catppuccin Mocha (catppuccin.com): base, text, subtext0 and the accents, all as published.
const CATPPUCCIN_MOCHA: Palette = Palette {
    background: hex(0x1E1E2E),
    text: hex(0xCDD6F4),
    secondary: hex(0xA6ADC8),
    tertiary: hex(0x7F849C),
    accent: hex(0xCBA6F7),
    link: hex(0x89B4FA),
    link_dim: hex(0x7F849C),
    selection: hex(0x45475A),
    key: hex(0x89B4FA),
    string: hex(0xA6E3A1),
    number: hex(0xFAB387),
    keyword: hex(0xCBA6F7),
    type_name: hex(0xF9E2AF),
    function: hex(0x89DCEB),
    comment: hex(0x9399B2),
    diff_add: hex(0xA6E3A1),
    diff_add_bg: hex(0x263A2E),
    diff_remove: hex(0xF38BA8),
    diff_remove_bg: hex(0x3B2633),
    dark: true,
};

/// Tokyo Night (enkia): the Night variant. The comment #565F89 is lightened (2.8:1).
const TOKYO_NIGHT: Palette = Palette {
    background: hex(0x1A1B26),
    text: hex(0xC0CAF5),
    secondary: hex(0xA9B1D6),
    tertiary: hex(0x737AA2),
    accent: hex(0x7AA2F7),
    link: hex(0x7DCFFF),
    link_dim: hex(0x6A8BC9),
    selection: hex(0x33467C),
    key: hex(0x7AA2F7),
    string: hex(0x9ECE6A),
    number: hex(0xFF9E64),
    keyword: hex(0xBB9AF7),
    type_name: hex(0x2AC3DE),
    function: hex(0x7DCFFF),
    comment: hex(0x8189B5),
    diff_add: hex(0x9ECE6A),
    diff_add_bg: hex(0x20303B),
    diff_remove: hex(0xF7768E),
    diff_remove_bg: hex(0x37222C),
    dark: true,
};

/// CopyCraft's own deep sea blue, with teal and cyan accents.
const OCEAN: Palette = Palette {
    background: hex(0x06324F),
    text: hex(0xEAF7FF),
    secondary: hex(0xA9D3EA),
    tertiary: hex(0x6FA3C2),
    accent: hex(0x3EE0F0),
    link: hex(0x5CC8FF),
    link_dim: hex(0x4E9CC9),
    selection: hex(0x0E5C86),
    key: hex(0x5CC8FF),
    string: hex(0x5EF2C4),
    number: hex(0xFFC46B),
    keyword: hex(0xC9A6FF),
    type_name: hex(0x3EE0F0),
    function: hex(0x8FD9FF),
    comment: hex(0x93BCD3),
    diff_add: hex(0x5EF2C4),
    diff_add_bg: hex(0x0A4A4E),
    diff_remove: hex(0xFF9C9C),
    diff_remove_bg: hex(0x3E2B48),
    dark: true,
};

/// CopyCraft's own deep forest green, with bright leaf and lime accents.
const FOREST: Palette = Palette {
    background: hex(0x0F3A24),
    text: hex(0xEEF9EF),
    secondary: hex(0xB3D9BC),
    tertiary: hex(0x75A884),
    accent: hex(0x7CF08C),
    link: hex(0x8FE3FF),
    link_dim: hex(0x6FB5A0),
    selection: hex(0x1E6640),
    key: hex(0x8FE3FF),
    string: hex(0xC4F27A),
    number: hex(0xFFC870),
    keyword: hex(0xF2A6E0),
    type_name: hex(0x7CF0C8),
    function: hex(0xA7F59A),
    comment: hex(0x9CC7A6),
    diff_add: hex(0xC4F27A),
    diff_add_bg: hex(0x1C4A20),
    diff_remove: hex(0xFFA8A0),
    diff_remove_bg: hex(0x43302A),
    dark: true,
};
#[cfg(test)]
mod tests {
    use super::{CardTheme, Palette, Rgb, contrast};

    /// Colours that must read as body text on the background, by name.
    fn body_colours(p: &Palette) -> [(&'static str, Rgb); 12] {
        [
            ("text", p.text),
            ("secondary", p.secondary),
            ("accent", p.accent),
            ("link", p.link),
            ("key", p.key),
            ("string", p.string),
            ("number", p.number),
            ("keyword", p.keyword),
            ("type", p.type_name),
            ("function", p.function),
            ("comment", p.comment),
            ("diff_add", p.diff_add),
        ]
    }

    fn fixed() -> impl Iterator<Item = (CardTheme, Palette)> {
        CardTheme::ALL
            .into_iter()
            .filter_map(|theme| theme.palette().map(|palette| (theme, palette)))
    }

    #[test]
    fn only_system_has_no_fixed_palette() {
        for theme in CardTheme::ALL {
            assert_eq!(theme.palette().is_none(), theme == CardTheme::System);
        }
        assert_eq!(fixed().count(), 15);
    }

    #[test]
    fn text_meets_aa_and_high_contrast_meets_aaa() {
        for (theme, p) in fixed() {
            let ratio = contrast(p.text, p.background);
            assert!(ratio >= 4.5, "{theme:?}: text {ratio:.2}");
            assert!(ratio >= theme.min_contrast(), "{theme:?}: text {ratio:.2}");
        }
        let hc = CardTheme::HighContrast.palette().expect("palette");
        assert!(contrast(hc.text, hc.background) >= 7.0);
    }

    #[test]
    fn secondary_code_and_diff_colours_read_on_the_background() {
        for (theme, p) in fixed() {
            let min = theme.min_contrast();
            let mut colours = body_colours(&p).to_vec();
            colours.push(("diff_remove", p.diff_remove));
            for (name, colour) in colours {
                let ratio = contrast(colour, p.background);
                assert!(ratio >= min, "{theme:?}: {name} {ratio:.2} < {min}");
            }
        }
    }

    #[test]
    fn dim_colours_meet_the_large_text_minimum() {
        for (theme, p) in fixed() {
            let min = if theme == CardTheme::HighContrast {
                7.0
            } else {
                3.0
            };
            for (name, colour) in [("tertiary", p.tertiary), ("link_dim", p.link_dim)] {
                let ratio = contrast(colour, p.background);
                assert!(ratio >= min, "{theme:?}: {name} {ratio:.2} < {min}");
            }
        }
    }

    #[test]
    fn diff_rows_and_selection_keep_their_text_readable() {
        for (theme, p) in fixed() {
            let min = theme.min_contrast();
            for (name, fg, bg) in [
                ("add on its row", p.diff_add, p.diff_add_bg),
                ("remove on its row", p.diff_remove, p.diff_remove_bg),
                ("text on the add row", p.text, p.diff_add_bg),
                ("text on the remove row", p.text, p.diff_remove_bg),
                ("text on the selection", p.text, p.selection),
            ] {
                let ratio = contrast(fg, bg);
                assert!(ratio >= min, "{theme:?}: {name} {ratio:.2} < {min}");
            }
        }
    }

    #[test]
    fn strip_tiles_and_the_table_grid_keep_aa_and_high_contrast_aaa() {
        for (theme, p) in fixed() {
            let min = theme.min_contrast();
            assert!(min >= 4.5);
            for current in [false, true] {
                let tile = p.strip_tile(current);
                for (name, fg) in [("tile text", tile.text), ("tile mark", tile.mark)] {
                    let ratio = contrast(fg, tile.background);
                    assert!(
                        ratio >= min,
                        "{theme:?}: {name} (current {current}) {ratio:.2} < {min}"
                    );
                }
            }
            for (name, fg, bg) in [
                ("grid text on the even rows", p.text, p.background),
                ("grid text on the odd rows", p.text, p.table_stripe()),
                ("grid header", p.accent, p.background),
            ] {
                let ratio = contrast(fg, bg);
                assert!(ratio >= min, "{theme:?}: {name} {ratio:.2} < {min}");
            }
            assert_ne!(p.table_stripe(), p.background, "{theme:?}: stripes show");
        }
        let hc = CardTheme::HighContrast.palette().expect("palette");
        assert!(contrast(hc.text, hc.strip_tile(true).background) >= 7.0);
    }

    #[test]
    fn mixing_moves_each_channel_toward_the_other_colour() {
        assert_eq!(
            Rgb(0, 100, 200).mix(Rgb(100, 100, 0), 0.5),
            Rgb(50, 100, 100)
        );
        assert_eq!(
            Rgb(10, 20, 30).mix(Rgb(255, 255, 255), 0.0),
            Rgb(10, 20, 30)
        );
    }

    #[test]
    fn dark_flag_matches_the_background() {
        for (theme, p) in fixed() {
            let luminance = mac_ui::style::luminance(p.background.unit());
            assert_eq!(p.dark, luminance < 0.2, "{theme:?}");
        }
    }

    #[test]
    fn night_is_the_icon_navy() {
        let night = CardTheme::Night.palette().expect("palette");
        assert_eq!(night.background.unit(), mac_ui::style::ACCENT_RGB);
    }

    #[test]
    fn stored_values_and_popup_positions_roundtrip() {
        for (index, theme) in CardTheme::ALL.into_iter().enumerate() {
            assert_eq!(CardTheme::parse(theme.as_str()), Some(theme));
            assert_eq!(theme.index(), index);
            assert_eq!(CardTheme::at(index), theme);
        }
        assert_eq!(CardTheme::parse("purple"), None);
        assert_eq!(CardTheme::at(99), CardTheme::System);
        assert_eq!(CardTheme::default(), CardTheme::System);
    }

    #[test]
    fn the_popup_groups_follow_each_other() {
        let groups: Vec<usize> = CardTheme::ALL.iter().map(|theme| theme.group()).collect();
        assert!(
            groups.windows(2).all(|pair| pair[0] <= pair[1]),
            "{groups:?}"
        );
        assert_eq!(groups.first(), Some(&0));
        assert_eq!(groups.last(), Some(&2));
        // Every stored value is distinct, so no theme shadows another.
        let mut values: Vec<&str> = CardTheme::ALL.iter().map(|theme| theme.as_str()).collect();
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), CardTheme::ALL.len());
    }

    #[test]
    fn the_colourful_themes_keep_the_published_colours_that_pass() {
        // The well-known palettes keep their background and body text exactly.
        for (theme, background, text) in [
            (CardTheme::Dracula, 0x282A36, 0xF8F8F2),
            (CardTheme::Nord, 0x2E3440, 0xECEFF4),
            (CardTheme::GruvboxDark, 0x282828, 0xEBDBB2),
            (CardTheme::Monokai, 0x272822, 0xF8F8F2),
            (CardTheme::CatppuccinMocha, 0x1E1E2E, 0xCDD6F4),
            (CardTheme::TokyoNight, 0x1A1B26, 0xC0CAF5),
        ] {
            let p = theme.palette().expect("palette");
            assert_eq!(p.background, super::hex(background), "{theme:?}");
            assert_eq!(p.text, super::hex(text), "{theme:?}");
            assert!(p.dark, "{theme:?}");
        }
    }
}
