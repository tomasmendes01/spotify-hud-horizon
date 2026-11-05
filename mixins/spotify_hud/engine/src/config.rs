// The settings and theme, kept as JSON in the mod's folder
// (`horizon-mods/.horizon-mod-data/spotify_hud/`).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// The edge or corner the box sticks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    TopMiddle,
    TopRight,
    MiddleLeft,
    MiddleRight,
    BottomLeft,
    BottomMiddle,
    BottomRight,
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Start,
    Middle,
    End,
}

impl Align {
    fn place(self, screen: f32, size: f32, offset: f32) -> f32 {
        match self {
            Align::Start => offset,
            Align::Middle => ((screen - size) / 2.0).floor() + offset,
            Align::End => screen - size + offset,
        }
    }
    fn offset(self, screen: f32, size: f32, at: f32) -> f32 {
        match self {
            Align::Start => at,
            Align::Middle => at - ((screen - size) / 2.0).floor(),
            Align::End => at - (screen - size),
        }
    }
}

impl Anchor {
    pub const ALL: [Anchor; 8] = [
        Anchor::TopLeft,
        Anchor::TopMiddle,
        Anchor::TopRight,
        Anchor::MiddleLeft,
        Anchor::MiddleRight,
        Anchor::BottomLeft,
        Anchor::BottomMiddle,
        Anchor::BottomRight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Anchor::TopLeft => "Top Left",
            Anchor::TopMiddle => "Top Middle",
            Anchor::TopRight => "Top Right",
            Anchor::MiddleLeft => "Middle Left",
            Anchor::MiddleRight => "Middle Right",
            Anchor::BottomLeft => "Bottom Left",
            Anchor::BottomMiddle => "Bottom Middle",
            Anchor::BottomRight => "Bottom Right",
        }
    }
    fn aligns(self) -> (Align, Align) {
        use Align::*;
        match self {
            Anchor::TopLeft => (Start, Start),
            Anchor::TopMiddle => (Middle, Start),
            Anchor::TopRight => (End, Start),
            Anchor::MiddleLeft => (Start, Middle),
            Anchor::MiddleRight => (End, Middle),
            Anchor::BottomLeft => (Start, End),
            Anchor::BottomMiddle => (Middle, End),
            Anchor::BottomRight => (End, End),
        }
    }
    /// The box's left edge, kept on screen.
    pub fn x(self, screen: f32, width: f32, offset: f32) -> f32 {
        self.aligns().0.place(screen, width, offset).clamp(0.0, (screen - width).max(0.0))
    }
    /// The box's top edge, kept on screen.
    pub fn y(self, screen: f32, height: f32, offset: f32) -> f32 {
        self.aligns().1.place(screen, height, offset).clamp(0.0, (screen - height).max(0.0))
    }
    /// The offset that puts the box's left edge at `at` (the position editor).
    pub fn x_offset(self, screen: f32, width: f32, at: f32) -> f32 {
        self.aligns().0.offset(screen, width, at)
    }
    pub fn y_offset(self, screen: f32, height: f32, at: f32) -> f32 {
        self.aligns().1.offset(screen, height, at)
    }
}

/// When the box shows, by the screen that is open ("Render Type").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderType {
    EscOnly,
    InGameOnly,
    #[default]
    NonIntrusive,
    Always,
    Inventory,
    EscInventory,
    TabList,
}

impl RenderType {
    pub const ALL: [RenderType; 7] = [
        RenderType::EscOnly,
        RenderType::InGameOnly,
        RenderType::NonIntrusive,
        RenderType::Always,
        RenderType::Inventory,
        RenderType::EscInventory,
        RenderType::TabList,
    ];
    pub fn label(self) -> &'static str {
        match self {
            RenderType::EscOnly => "Pause menu",
            RenderType::InGameOnly => "Playing only",
            RenderType::NonIntrusive => "Playing, pause & chat",
            RenderType::Always => "Everywhere",
            RenderType::Inventory => "Inventory",
            RenderType::EscInventory => "Pause & inventory",
            RenderType::TabList => "Holding Tab",
        }
    }
    /// `screen`: the id of the open screen (`paused`, `chat`, `inventory`,
    /// `creative`…), None while playing. `debug`: F3's screen is up.
    pub fn shows(self, screen: Option<&str>, debug: bool, tab: bool) -> bool {
        match self {
            RenderType::EscOnly => screen == Some("paused"),
            RenderType::InGameOnly => screen.is_none() && !debug,
            RenderType::NonIntrusive => match screen {
                None | Some("paused") => !debug,
                Some(s) => s == "chat",
            },
            RenderType::Always => screen.is_some() || !debug,
            RenderType::Inventory => screen == Some("inventory"),
            RenderType::EscInventory => matches!(screen, Some("paused" | "inventory" | "creative")),
            RenderType::TabList => tab,
        }
    }
}

/// When the box shows, by what is playing ("Display Mode").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DisplayMode {
    AllTheTime,
    WhenPlaying,
    #[default]
    WhenSongFound,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 3] = [DisplayMode::AllTheTime, DisplayMode::WhenPlaying, DisplayMode::WhenSongFound];
    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::AllTheTime => "Always",
            DisplayMode::WhenPlaying => "While playing",
            DisplayMode::WhenSongFound => "Once a song is known",
        }
    }
}

/// What the link button does ("Link Mode").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LinkMode {
    #[default]
    Open,
    Copy,
    Chat,
}

impl LinkMode {
    pub const ALL: [LinkMode; 3] = [LinkMode::Open, LinkMode::Copy, LinkMode::Chat];
    pub fn label(self) -> &'static str {
        match self {
            LinkMode::Open => "Open in browser",
            LinkMode::Copy => "Copy",
            LinkMode::Chat => "Show in chat",
        }
    }
}

/// The next value of `all` after `v` (backwards with `back`).
pub fn cycle<T: Copy + PartialEq>(all: &[T], v: T, back: bool) -> T {
    let i = all.iter().position(|x| *x == v).unwrap_or(0);
    let n = all.len();
    all[if back { (i + n - 1) % n } else { (i + 1) % n }]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub first_time: bool,
    /// "spotify" or "disabled".
    pub music_service: String,
    pub link_mode: LinkMode,
    pub announcement_enabled: bool,
    pub announcement_message: String,
    pub anchor_point: Anchor,
    pub render_type: RenderType,
    pub display_mode: DisplayMode,
    /// "Controls": the buttons under the box when hovered (needs Spotify Premium).
    pub premium_control: bool,
    pub x_offset: f32,
    pub y_offset: f32,
    /// 1 to 5.
    pub marquee_speed: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            first_time: true,
            music_service: "disabled".into(),
            link_mode: LinkMode::Open,
            announcement_enabled: false,
            announcement_message: "&a♪ &7${song} &8- &b${artists}".into(),
            anchor_point: Anchor::TopLeft,
            render_type: RenderType::NonIntrusive,
            display_mode: DisplayMode::WhenSongFound,
            premium_control: true,
            x_offset: 0.0,
            y_offset: 0.0,
            marquee_speed: 5,
        }
    }
}

/// A colour as `#AARRGGBB` (or `#RRGGBB`, opaque) in the JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argb(pub u32);

impl Serialize for Argb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("#{:08X}", self.0))
    }
}

impl<'de> Deserialize<'de> for Argb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_colour(&s).map(Argb).ok_or_else(|| serde::de::Error::custom(format!("not a colour: {s}")))
    }
}

pub fn parse_colour(s: &str) -> Option<u32> {
    let h = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        6 => Some(0xFF00_0000 | v),
        8 => Some(v),
        _ => None,
    }
}

const fn rgb(r: u32, g: u32, b: u32) -> Argb {
    Argb(0xFF00_0000 | r << 16 | g << 8 | b)
}

/// its theme: colours, radii and which control buttons show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub border_color: Argb,
    pub title_color: Argb,
    pub artist_color: Argb,
    pub background_color: Argb,
    pub background_radius: f32,
    pub progress_background_color: Argb,
    pub progress_color: Argb,
    pub progress_number_color: Argb,
    pub progress_radius: f32,
    pub control_color: Argb,
    pub hover_control_color: Argb,
    pub selected_control_color: Argb,
    pub selected_hover_control_color: Argb,
    pub show_position_editor_button: bool,
    pub show_settings_button: bool,
    pub show_shuffle_button: bool,
    pub show_previous_button: bool,
    pub show_play_button: bool,
    pub show_next_button: bool,
    pub show_repeat_button: bool,
    pub show_external_button: bool,
    pub show_volume_button: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_color: rgb(1, 165, 82),
            title_color: rgb(255, 255, 255),
            artist_color: rgb(255, 255, 255),
            background_color: Argb(0x7800_0000),
            background_radius: 0.0,
            progress_background_color: rgb(50, 50, 50),
            progress_color: rgb(255, 255, 255),
            progress_number_color: rgb(255, 255, 255),
            progress_radius: 3.0,
            control_color: rgb(255, 255, 255),
            hover_control_color: rgb(150, 150, 150),
            selected_control_color: rgb(1, 165, 82),
            selected_hover_control_color: rgb(0, 212, 105),
            show_position_editor_button: true,
            show_settings_button: true,
            show_shuffle_button: true,
            show_previous_button: true,
            show_play_button: true,
            show_next_button: true,
            show_repeat_button: true,
            show_external_button: true,
            show_volume_button: true,
        }
    }
}

/// Reads `name` from `dir`, the default when it is missing or broken.
pub fn load<T: Default + for<'de> Deserialize<'de>>(dir: &Path, name: &str) -> T {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Writes `value` to `dir/name`, through a temporary file.
pub fn save<T: Serialize>(dir: &Path, name: &str, value: &T) {
    let Ok(json) = serde_json::to_string_pretty(value) else { return };
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(format!("{name}.tmp"));
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_clamp_and_invert() {
        let a = Anchor::BottomRight;
        assert_eq!(a.x(400.0, 150.0, 0.0), 250.0);
        assert_eq!(a.y(240.0, 50.0, 0.0), 190.0);
        assert_eq!(a.x(400.0, 150.0, 30.0), 250.0, "kept on screen");
        let off = a.x_offset(400.0, 150.0, 100.0);
        assert_eq!(a.x(400.0, 150.0, off), 100.0);
        let m = Anchor::TopMiddle;
        assert_eq!(m.x(400.0, 150.0, 0.0), 125.0);
        assert_eq!(m.x(400.0, 150.0, m.x_offset(400.0, 150.0, 10.0)), 10.0);
    }

    #[test]
    fn non_intrusive_is_cratifys() {
        let r = RenderType::NonIntrusive;
        assert!(r.shows(None, false, false));
        assert!(!r.shows(None, true, false));
        assert!(r.shows(Some("paused"), false, false));
        assert!(r.shows(Some("chat"), true, false));
        assert!(!r.shows(Some("inventory"), false, false));
    }

    #[test]
    fn colours_round_trip() {
        let t = Theme::default();
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"#7800000"));
        let back: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        assert_eq!(parse_colour("#01A552"), Some(0xFF01_A552));
        let partial: Theme = serde_json::from_str(r##"{"title_color":"#FF0000"}"##).unwrap();
        assert_eq!(partial.title_color, Argb(0xFFFF_0000));
        assert_eq!(partial.border_color, Theme::default().border_color);
    }

    #[test]
    fn cycles_wrap() {
        assert_eq!(cycle(&DisplayMode::ALL, DisplayMode::WhenSongFound, false), DisplayMode::AllTheTime);
        assert_eq!(cycle(&DisplayMode::ALL, DisplayMode::AllTheTime, true), DisplayMode::WhenSongFound);
    }
}
