// Spotify HUD for Horizon: the song Spotify is playing in a box on the HUD, with controls.
// Inspired by ThatGravyBoat's Craftify; written from scratch for Horizon's native mods. This
// crate is everything but the game glue: the Spotify service, its login, the settings, and what
// the HUD box and the mod's screens draw and do when clicked. The bundle's helpers call it from
// Horizon's hook points.

pub mod art;
pub mod config;
pub mod draw;
pub mod spotify;

use config::{Anchor, Config, DisplayMode, LinkMode, RenderType, Theme, cycle};
use draw::{Canvas, Cmd, Width, inside};
use spotify::{Action, Repeat, Service, State};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const ID: &str = "spotify_hud";
pub const NAME: &str = "Spotify HUD";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str = "The song Spotify is playing, in a corner of the HUD: cover, progress and buttons to play, pause, skip, shuffle, repeat and change the volume. Type /music in a world.";

/// The mod's keys: name, label and default (unbound).
pub const KEYS: [(&str, &str); 4] = [
    ("skip_forward", "Next song"),
    ("skip_previous", "Previous song"),
    ("toggle_playing", "Play / pause"),
    ("toggle_hud", "Show / hide the box"),
];
pub const UNBOUND: &str = "key.keyboard.unknown";
/// The chat command, without its slash.
pub const COMMAND: &str = "music";

pub const CLIENT_ID_HELP: &[&str] = &[
    "§aSpotify HUD §8» §7First, a Spotify app of your own (one time only):",
    "§7 1. Go to §9developer.spotify.com/dashboard§7 and press §fCreate app§7.",
    "§7 2. Add the redirect URI §fhttp://127.0.0.1:21851/spotify/callback§7, tick §fWeb API§7, save.",
    "§7 3. Paste its §fClient ID§7 here: §9/music clientid <Client ID>§7.",
    "§7 4. Then §9/music login§7, or the Account tab of §9/music§7.",
];

const PADDING: f32 = 5.0;
const WIDTH: f32 = 150.0;
const HEIGHT: f32 = 50.0;
const FOCUSED_HEIGHT: f32 = 63.0;

/// The player's control buttons, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    PositionEditor,
    Settings,
    Shuffle,
    Previous,
    Play,
    Next,
    Repeat,
    External,
    Volume,
}

impl Control {
    pub const ALL: [Control; 9] = [
        Control::PositionEditor,
        Control::Settings,
        Control::Shuffle,
        Control::Previous,
        Control::Play,
        Control::Next,
        Control::Repeat,
        Control::External,
        Control::Volume,
    ];
    fn shown(self, t: &Theme) -> bool {
        match self {
            Control::PositionEditor => t.show_position_editor_button,
            Control::Settings => t.show_settings_button,
            Control::Shuffle => t.show_shuffle_button,
            Control::Previous => t.show_previous_button,
            Control::Play => t.show_play_button,
            Control::Next => t.show_next_button,
            Control::Repeat => t.show_repeat_button,
            Control::External => t.show_external_button,
            Control::Volume => t.show_volume_button,
        }
    }
    fn shown_mut(self, t: &mut Theme) -> &mut bool {
        match self {
            Control::PositionEditor => &mut t.show_position_editor_button,
            Control::Settings => &mut t.show_settings_button,
            Control::Shuffle => &mut t.show_shuffle_button,
            Control::Previous => &mut t.show_previous_button,
            Control::Play => &mut t.show_play_button,
            Control::Next => &mut t.show_next_button,
            Control::Repeat => &mut t.show_repeat_button,
            Control::External => &mut t.show_external_button,
            Control::Volume => &mut t.show_volume_button,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Control::PositionEditor => "Move button",
            Control::Settings => "Settings button",
            Control::Shuffle => "Shuffle button",
            Control::Previous => "Previous button",
            Control::Play => "Play/pause button",
            Control::Next => "Next button",
            Control::Repeat => "Repeat button",
            Control::External => "Song link button",
            Control::Volume => "Volume button",
        }
    }
}

/// One line of the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    MusicService,
    LinkMode,
    Announcement,
    AnnouncementMessage,
    Folder,
    AnchorPoint,
    RenderType,
    DisplayMode,
    Controls,
    MarqueeSpeed,
    PositionEditor,
    Show(Control),
    ReloadTheme,
    ClientId,
    Login,
    Logout,
    Restart,
    Volume,
}

pub const TABS: [&str; 4] = ["General", "Display", "Look", "Account"];

fn tab_items(tab: usize) -> Vec<Item> {
    match tab {
        0 => vec![Item::MusicService, Item::LinkMode, Item::Announcement, Item::AnnouncementMessage, Item::Folder],
        1 => vec![Item::AnchorPoint, Item::RenderType, Item::DisplayMode, Item::Controls, Item::MarqueeSpeed, Item::PositionEditor],
        2 => {
            let mut v: Vec<Item> = Control::ALL.iter().map(|c| Item::Show(*c)).collect();
            v.push(Item::ReloadTheme);
            v
        }
        _ => vec![Item::ClientId, Item::Login, Item::Logout, Item::Restart, Item::Volume],
    }
}

/// What Options > Mods lists on its page: everything but the theme.
const PAGE_ITEMS: [Item; 15] = [
    Item::MusicService,
    Item::LinkMode,
    Item::Announcement,
    Item::AnchorPoint,
    Item::RenderType,
    Item::DisplayMode,
    Item::Controls,
    Item::MarqueeSpeed,
    Item::ClientId,
    Item::Login,
    Item::Logout,
    Item::Restart,
    Item::ReloadTheme,
    Item::AnnouncementMessage,
    Item::Folder,
];

#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Settings { tab: usize },
    Volume { volume: f32, last: f32, dragging: bool },
    Position { grab: Option<[f32; 2]> },
}

/// What the HUD box drew last: where, how tall and its buttons, for clicks.
#[derive(Clone, Debug, Default)]
struct Drawn {
    rect: [f32; 4],
    hovered: bool,
    buttons: Vec<(Control, [f32; 4])>,
    visible: bool,
}

pub struct SpotifyHud {
    dir: PathBuf,
    pub config: Config,
    pub theme: Theme,
    service: Service,
    covers: art::Covers,
    /// the original's own screen while it is open (or opening).
    pub screen: Option<Screen>,
    /// A screen to open as soon as the game lets it (after chat closes).
    pub opening: Option<(Screen, u32)>,
    temporarily_hidden: bool,
    chat: Vec<String>,
    drawn: Drawn,
    box_height: f32,
    /// The HUD's ticks (20 a second) for the marquee.
    ticks: f32,
    /// The open screen's id as the last `screen_render_post` saw it, and that frame.
    seen_screen: Option<(u64, String)>,
    frame: u64,
    tab_down: bool,
    announced: Option<String>,
    /// The GUI's size when last drawn, for the input hooks that don't get it.
    pub last_gui: [f32; 2],
}

static HUD: OnceLock<Mutex<SpotifyHud>> = OnceLock::new();

/// The mod's state, started on first use with its folder.
pub fn get(dir: impl FnOnce() -> PathBuf) -> MutexGuard<'static, SpotifyHud> {
    HUD
        .get_or_init(|| Mutex::new(SpotifyHud::new(dir())))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// The mod's state if it has started (hooks that can't name its folder).
pub fn started() -> Option<MutexGuard<'static, SpotifyHud>> {
    HUD.get().map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The game's view of the frame, for the HUD and screens.
#[derive(Clone, Copy, Debug, Default)]
pub struct View<'a> {
    /// The GUI's size in GUI pixels.
    pub gui: [f32; 2],
    /// Screen pixels per GUI pixel.
    pub scale: f32,
    /// The pointer, in GUI pixels.
    pub mouse: [f32; 2],
    /// The open screen's id, None while playing.
    pub screen: Option<&'a str>,
    /// F3's screen is up.
    pub debug: bool,
    /// F1: the GUI is hidden.
    pub gui_hidden: bool,
    /// In a world.
    pub in_world: bool,
}

fn state_label(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

fn format_time(ms: f32) -> String {
    let s = (ms / 1000.0).max(0.0);
    format!("{:02}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32)
}

impl SpotifyHud {
    fn new(dir: PathBuf) -> Self {
        let config: Config = config::load(&dir, "config.json");
        let theme: Theme = config::load(&dir, "theme.json");
        if !dir.join("theme.json").exists() {
            config::save(&dir, "theme.json", &theme);
        }
        let service = Service::new(dir.clone());
        if config.music_service == "spotify" {
            service.send(Action::Start);
        }
        Self {
            dir,
            config,
            theme,
            service,
            covers: art::Covers::default(),
            screen: None,
            opening: None,
            temporarily_hidden: false,
            chat: Vec::new(),
            drawn: Drawn::default(),
            box_height: HEIGHT,
            ticks: 0.0,
            seen_screen: None,
            frame: 0,
            tab_down: false,
            announced: None,
            last_gui: [427.0, 240.0],
        }
    }

    fn save(&self) {
        config::save(&self.dir, "config.json", &self.config);
    }

    fn say(&mut self, line: impl Into<String>) {
        self.chat.push(line.into());
    }

    fn state(&self) -> Option<(State, u64)> {
        let s = self.service.lock();
        s.state.as_ref().map(|(st, at)| (st.clone(), spotify::progress_now(st, *at)))
    }

    fn set_service(&mut self, id: &str) {
        self.config.music_service = id.into();
        self.save();
        self.service.send(if id == "spotify" { Action::Restart } else { Action::Stop });
    }

    // ---- once a tick and once a frame ----

    /// Once a client tick, in a world or not: chat lines to show (the caller sends them with
    /// `native_chat`), the first-time message and announcements.
    pub fn tick(&mut self, in_world: bool) -> Vec<String> {
        let (logged_in, mut lines, state) = {
            let mut s = self.service.lock();
            let logged_in = std::mem::take(&mut s.logged_in);
            (logged_in, std::mem::take(&mut s.chat), s.state.as_ref().map(|x| x.0.clone()))
        };
        if logged_in && self.config.music_service != "spotify" {
            self.config.music_service = "spotify".into();
            self.save();
        }
        if in_world && self.config.first_time {
            self.config.first_time = false;
            self.save();
            lines.extend(
                [
                    "§aSpotify HUD §8» §7Welcome! Type §9/music§7 and open the §fAccount§7 tab to connect Spotify.",
                ]
                .map(String::from),
            );
        }
        if let Some(state) = state {
            if self.announced.as_deref() != Some(state.song.title.as_str()) {
                self.announced = Some(state.song.title.clone());
                if self.config.announcement_enabled {
                    let artists = state.song.artists.join(", ");
                    let artist = state.song.artists.first().cloned().unwrap_or_default();
                    lines.push(
                        self.config
                            .announcement_message
                            .replace('&', "§")
                            .replace("${song}", &state.song.title)
                            .replace("${artists}", &artists)
                            .replace("${artist}", &artist),
                    );
                }
            }
        }
        lines.append(&mut self.chat);
        if !in_world {
            // Chat shows nothing outside a world: keep them for it (a few).
            self.chat = lines;
            let n = self.chat.len();
            self.chat.drain(..n.saturating_sub(20));
            return Vec::new();
        }
        lines
    }

    /// Once a frame: the marquee's clock, the tab list key's state (read with
    /// `native_mapping`, which needs the game mutably), and whether the original's own screen is
    /// still the one open (Escape closes it without telling the mod).
    pub fn frame(&mut self, elapsed_secs: f32, tab_down: bool, own_screen: bool) {
        self.frame += 1;
        self.ticks += elapsed_secs.min(1.0) * 20.0;
        self.tab_down = tab_down;
        if !own_screen && self.opening.is_none() {
            self.screen = None;
        }
    }

    /// The game drew its open screen (`screen_render_post`): remember which.
    pub fn saw_screen(&mut self, id: &str) {
        self.seen_screen = Some((self.frame, id.to_owned()));
    }

    /// The id of the screen open this frame, None while playing.
    pub fn open_screen(&self) -> Option<&str> {
        self.seen_screen.as_ref().filter(|(f, _)| *f + 1 >= self.frame).map(|(_, s)| s.as_str())
    }

    /// A screen to open (`opening`), taken by the frame hook when it opens it.
    pub fn take_opening(&mut self) -> Option<Screen> {
        let (screen, tries) = self.opening.take()?;
        if tries < 40 {
            self.opening = Some((screen.clone(), tries + 1));
        }
        Some(screen)
    }

    /// The frame hook opened its screen.
    pub fn opened(&mut self, screen: Screen) {
        self.opening = None;
        self.screen = Some(screen);
    }

    fn open(&mut self, screen: Screen) {
        self.opening = Some((screen, 0));
    }

    fn volume_screen(&self) -> Screen {
        let v = self.state().map(|(s, _)| f32::from(s.volume) / 100.0).unwrap_or(0.25).clamp(0.0, 1.0);
        Screen::Volume { volume: v, last: v, dragging: false }
    }

    // ---- the HUD player ----

    /// Whether the box is hidden.
    fn hidden(&self, v: &View<'_>, own_screen: bool) -> bool {
        if self.temporarily_hidden || v.gui_hidden || self.config.music_service == "disabled" || !v.in_world {
            return true;
        }
        if own_screen {
            return true;
        }
        if !self.config.render_type.shows(v.screen, v.debug, self.tab_down) {
            return true;
        }
        let state = self.service.lock().state.as_ref().map(|s| s.0.playing);
        match self.config.display_mode {
            DisplayMode::AllTheTime => false,
            DisplayMode::WhenPlaying => state != Some(true),
            DisplayMode::WhenSongFound => state.is_none(),
        }
    }

    /// The HUD box over the GUI (`gui_render_post`), unless one of its screens is
    /// open, which draws it itself.
    pub fn hud(&mut self, v: &View<'_>, own_screen: bool, width: Width<'_>) -> Vec<Cmd> {
        self.last_gui = v.gui;
        if self.hidden(v, own_screen) {
            self.drawn.visible = false;
            return Vec::new();
        }
        let mouse = v.screen.map(|_| v.mouse);
        self.player(v, mouse, width)
    }

    /// The box: cover, title, artists, times and progress, and the controls
    /// row while hovered.
    fn player(&mut self, v: &View<'_>, mouse: Option<[f32; 2]>, width: Width<'_>) -> Vec<Cmd> {
        let c = &self.config;
        let t = &self.theme;
        let x = c.anchor_point.x(v.gui[0], WIDTH, c.x_offset).floor();
        let y_before = c.anchor_point.y(v.gui[1], self.box_height, c.y_offset).floor();
        let hovered = mouse.is_some_and(|m| inside(m, [x, y_before, WIDTH, self.box_height]));
        let height = if hovered && c.premium_control { FOCUSED_HEIGHT } else { HEIGHT };
        self.box_height = height;
        let y = c.anchor_point.y(v.gui[1], height, c.y_offset).floor();

        let mut out = Canvas::default();
        let edge = if hovered { t.border_color.0 } else { t.background_color.0 };
        out.bordered([x, y, WIDTH, height], t.background_radius, t.background_color.0, edge, 1.0);

        let snapshot = self.state();
        let (state, progress) = match &snapshot {
            Some((s, p)) => (Some(s), *p as f32),
            None => (None, 0.0),
        };
        let song_title = state.map_or("No song playing", |s| s.song.title.as_str());
        let artists_list: &[String] = state.map_or(&[], |s| &s.song.artists);
        let mut artists = artists_list.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
        if artists_list.len() > 3 {
            artists.push_str(", ...");
        }

        // The cover, one cell per screen pixel; the background colour while it loads.
        let (ix, iy) = (x + PADDING, y + PADDING);
        let side = (40.0 * v.scale).round().clamp(10.0, art::SIDE as f32) as usize;
        match state.and_then(|s| self.covers.sized(&s.song.cover, side)) {
            Some(px) => out.image(&px, side, [ix, iy], 40.0),
            None => out.rounded([ix, iy, 40.0, 40.0], t.background_radius, t.background_color.0),
        }

        let speed = (6 - i32::from(c.marquee_speed.clamp(1, 5))).max(1) as f32;
        let ticks = self.ticks / speed;
        let tx = x + PADDING + 40.0 + PADDING;
        out.text_in(width, song_title, [tx, y + PADDING, 95.0, 10.0], 0.0, t.title_color.0, 1.0, Some(ticks));
        out.text_in(width, &artists, [tx, y + PADDING + 12.0, 95.0, 6.0], 0.0, t.artist_color.0, 0.5, Some(ticks));

        let total = state.map_or(0.0, |s| s.duration_ms as f32);
        let time = progress.min(if total > 0.0 { total } else { progress });
        let py = y + PADDING + 29.0;
        out.text_in(width, &format_time(time), [tx, py, 30.0, 7.0], 0.0, t.progress_number_color.0, 0.5, None);
        out.text_in(width, &format_time(total), [tx + 65.0, py, 30.0, 7.0], 1.0, t.progress_number_color.0, 0.5, None);
        let pct = if total > 0.0 { (time / total).clamp(0.0, 1.0) } else { 0.0 };
        out.rounded([tx, py + 8.0, 95.0, 3.0], t.progress_radius, t.progress_background_color.0);
        out.rounded([tx, py + 8.0, (95.0 * pct).floor(), 3.0], t.progress_radius, t.progress_color.0);

        let mut buttons = Vec::new();
        if hovered && c.premium_control {
            let shown: Vec<Control> = Control::ALL.into_iter().filter(|b| b.shown(t)).collect();
            let n = shown.len() as f32;
            let row = n * 10.0 + (n - 1.0).max(0.0) * 2.0;
            let mut bx = x + PADDING + ((WIDTH - PADDING * 2.0 - row) / 2.0).floor();
            let by = y + PADDING + 44.0;
            for b in shown {
                let r = [bx, by, 10.0, 10.0];
                let over = mouse.is_some_and(|m| inside(m, r));
                let on = match b {
                    Control::Shuffle => state.is_some_and(|s| s.shuffle),
                    Control::Repeat => state.is_some_and(|s| s.repeat != Repeat::Off),
                    Control::Play => state.is_some_and(|s| s.playing),
                    _ => false,
                };
                // Play shows pause while playing, with the ordinary colours.
                let selectable = matches!(b, Control::Shuffle | Control::Repeat);
                let colour = match (over, on && selectable) {
                    (true, true) => t.selected_hover_control_color.0,
                    (false, true) => t.selected_control_color.0,
                    (true, false) => t.hover_control_color.0,
                    (false, false) => t.control_color.0,
                };
                let icon = match b {
                    Control::PositionEditor => &draw::ICON_POSITION,
                    Control::Settings => &draw::ICON_SETTINGS,
                    Control::Shuffle => &draw::ICON_SHUFFLE,
                    Control::Previous => &draw::ICON_PREVIOUS,
                    Control::Play if on => &draw::ICON_PAUSE,
                    Control::Play => &draw::ICON_PLAY,
                    Control::Next => &draw::ICON_NEXT,
                    Control::Repeat => &draw::ICON_REPEAT,
                    Control::External => &draw::ICON_EXTERNAL,
                    Control::Volume => &draw::ICON_VOLUME,
                };
                out.icon(icon, [bx, by], 10.0, colour);
                buttons.push((b, r));
                bx += 12.0;
            }
        }
        self.drawn = Drawn { rect: [x, y, WIDTH, height], hovered, buttons, visible: true };
        out.cmds
    }

    fn control(&mut self, b: Control) {
        let state = self.state().map(|s| s.0);
        match b {
            Control::PositionEditor => self.open(Screen::Position { grab: None }),
            Control::Settings => self.open(Screen::Settings { tab: 0 }),
            Control::Shuffle => self.service.send(Action::ToggleShuffle),
            Control::Previous => self.service.send(Action::Move(false)),
            Control::Next => self.service.send(Action::Move(true)),
            Control::Play => {
                if let Some(s) = &state {
                    self.service.send(Action::SetPaused(s.playing));
                }
            }
            Control::Repeat => self.service.send(Action::ToggleRepeat),
            Control::External => {
                if let Some(s) = state {
                    self.link(&s.song.url);
                }
            }
            Control::Volume => {
                let s = self.volume_screen();
                self.open(s);
            }
        }
    }

    fn link(&mut self, url: &str) {
        let done = match self.config.link_mode {
            LinkMode::Open => open::that_detached(url).is_ok(),
            LinkMode::Copy => arboard::Clipboard::new().and_then(|mut c| c.set_text(url.to_owned())).is_ok(),
            LinkMode::Chat => false,
        };
        if !done {
            self.say(format!("§aSpotify HUD §8» §7{url}"));
        }
    }

    // ---- input ----

    /// A mouse button went down over a screen (`screen_mouse_button_pressed_pre`): true when
    /// the mod took it.
    pub fn press(&mut self, v: &View<'_>, own_screen: bool, button: i32, width: Width<'_>) -> bool {
        if own_screen {
            return self.screen_press(v, button, width);
        }
        if !self.drawn.visible || !self.drawn.hovered || !self.config.premium_control {
            return false;
        }
        let hit = self.drawn.buttons.iter().find(|(_, r)| inside(v.mouse, *r)).map(|(b, _)| *b);
        match hit {
            Some(b) if button == 0 => {
                self.control(b);
                true
            }
            _ => false,
        }
    }

    pub fn release(&mut self, _v: &View<'_>) -> bool {
        match &mut self.screen {
            Some(Screen::Volume { volume, dragging, .. }) if *dragging => {
                *dragging = false;
                let v = (*volume * 100.0) as u8;
                self.service.send(Action::Volume(v));
                true
            }
            Some(Screen::Position { grab }) => {
                *grab = None;
                true
            }
            _ => false,
        }
    }

    pub fn drag(&mut self, v: &View<'_>) -> bool {
        match self.screen.clone() {
            Some(Screen::Volume { dragging: true, last, .. }) => {
                let (bx, bw) = volume_bar(v);
                let volume = ((v.mouse[0] - bx) / bw).clamp(0.0, 1.0);
                self.screen = Some(Screen::Volume { volume, last, dragging: true });
                true
            }
            Some(Screen::Position { grab: Some([dx, dy]) }) => {
                let a = self.config.anchor_point;
                self.config.x_offset = a.x_offset(v.gui[0], WIDTH, (v.mouse[0] - dx).floor());
                self.config.y_offset = a.y_offset(v.gui[1], self.box_height, (v.mouse[1] - dy).floor());
                self.save();
                true
            }
            _ => false,
        }
    }

    pub fn scroll(&mut self, dy: f64) -> bool {
        if let Some(Screen::Volume { volume, last, dragging }) = self.screen.clone() {
            let volume = (volume + dy.signum() as f32 * 0.05).clamp(0.0, 1.0);
            self.screen = Some(Screen::Volume { volume, last, dragging });
            self.service.send(Action::Volume((volume * 100.0).round() as u8));
            return true;
        }
        false
    }

    /// A key of the mod's went down while playing: `key` is the vanilla name and `bound`
    /// the player's choice for one of `KEYS`.
    pub fn key(&mut self, key: &str, bound: &dyn Fn(&str) -> String) {
        if key == UNBOUND {
            return;
        }
        let Some(name) = KEYS.iter().map(|k| k.0).find(|name| bound(name) == key) else { return };
        let playing = self.state().map(|s| s.0.playing);
        match name {
            "skip_forward" => self.service.send(Action::Move(true)),
            "skip_previous" => self.service.send(Action::Move(false)),
            "toggle_playing" => {
                if let Some(p) = playing {
                    self.service.send(Action::SetPaused(p));
                }
            }
            "toggle_hud" => self.temporarily_hidden = !self.temporarily_hidden,
            _ => {}
        }
    }

    // ---- the settings ----

    fn item_text(&self, item: Item) -> (String, String, &'static str) {
        let c = &self.config;
        let (name, value, desc): (&str, String, &str) = match item {
            Item::MusicService => (
                "Enabled",
                if c.music_service == "spotify" { "Spotify" } else { "Off" }.into(),
                "Turns the Spotify connection and the box on or off.",
            ),
            Item::LinkMode => ("Song link", c.link_mode.label().into(), "What the link button does with the song's Spotify page."),
            Item::Announcement => ("Now playing in chat", state_label(c.announcement_enabled).into(), "A line in chat each time the song changes."),
            Item::AnnouncementMessage => (
                "Chat line",
                "Edit".into(),
                "/music announcement <text>: ${song}, ${artists} and ${artist} become the song's; & starts a colour code.",
            ),
            Item::Folder => ("Mod folder", "Open".into(), "config.json, theme.json and the Spotify login live here."),
            Item::AnchorPoint => ("Corner", c.anchor_point.label().into(), "The edge or corner the box sticks to."),
            Item::RenderType => ("Show on", c.render_type.label().into(), "Which screens the box appears on."),
            Item::DisplayMode => ("Show when", c.display_mode.label().into(), "Whether the box needs music playing to appear."),
            Item::Controls => (
                "Buttons",
                state_label(c.premium_control).into(),
                "Buttons under the box when you point at it with chat open. Spotify only accepts them from Premium accounts.",
            ),
            Item::MarqueeSpeed => ("Scroll speed", c.marquee_speed.to_string(), "How fast long titles slide past (1 slow, 5 fast)."),
            Item::PositionEditor => ("Position", "Move".into(), "Drag the box wherever you like."),
            Item::Show(b) => (b.label(), state_label(b.shown(&self.theme)).into(), "Whether the button is in the row under the box."),
            Item::ReloadTheme => ("Colours", "Reload theme.json".into(), "Edit theme.json in the mod folder (#AARRGGBB colours), then reload it here."),
            Item::ClientId => {
                let id = self.service.lock().auth.client_id.clone();
                let shown = if id.is_empty() { "Not set".to_owned() } else { format!("{}…", id.chars().take(8).collect::<String>()) };
                ("Spotify app", shown, "Your Spotify app's Client ID, set with /music clientid <id>. Click for how.")
            }
            Item::Login => {
                let s = self.service.lock();
                let v = if s.login_pending {
                    "Waiting…"
                } else if s.auth.refresh.is_empty() {
                    "Connect"
                } else {
                    "Reconnect"
                };
                ("Spotify", v.into(), "Opens Spotify in your browser; you have two minutes to allow it.")
            }
            Item::Logout => ("Disconnect", "Forget login".into(), "Removes the saved Spotify login from this computer."),
            Item::Restart => {
                let phase = match self.service.lock().phase() {
                    spotify::Phase::Stopped => "Stopped",
                    spotify::Phase::Starting => "Starting",
                    spotify::Phase::Running => "Running",
                };
                ("Connection", format!("{phase} · Restart"), "Reconnects to Spotify (/music restart).")
            }
            Item::Volume => ("Volume", "Open".into(), "Spotify's volume (/music volume [0-100])."),
        };
        (name.to_owned(), value, desc)
    }

    fn click_item(&mut self, item: Item, back: bool) {
        let c = &mut self.config;
        match item {
            Item::MusicService => {
                let next = if c.music_service == "spotify" { "disabled" } else { "spotify" };
                self.set_service(next);
                return;
            }
            Item::LinkMode => c.link_mode = cycle(&LinkMode::ALL, c.link_mode, back),
            Item::Announcement => c.announcement_enabled = !c.announcement_enabled,
            Item::AnnouncementMessage => {
                let line = format!("§aSpotify HUD §8» §7Chat line: §f{}§7. Change it: §9/music announcement <text>", c.announcement_message);
                self.say(line);
                return;
            }
            Item::Folder => {
                let _ = std::fs::create_dir_all(&self.dir);
                let _ = open::that_detached(&self.dir);
                return;
            }
            Item::AnchorPoint => {
                c.anchor_point = cycle(&Anchor::ALL, c.anchor_point, back);
                c.x_offset = 0.0;
                c.y_offset = 0.0;
            }
            Item::RenderType => c.render_type = cycle(&RenderType::ALL, c.render_type, back),
            Item::DisplayMode => c.display_mode = cycle(&DisplayMode::ALL, c.display_mode, back),
            Item::Controls => c.premium_control = !c.premium_control,
            Item::MarqueeSpeed => {
                c.marquee_speed = if back { (c.marquee_speed + 3) % 5 + 1 } else { c.marquee_speed % 5 + 1 };
            }
            Item::PositionEditor => {
                self.open(Screen::Position { grab: None });
                return;
            }
            Item::Show(b) => {
                let v = b.shown_mut(&mut self.theme);
                *v = !*v;
                config::save(&self.dir, "theme.json", &self.theme);
                return;
            }
            Item::ReloadTheme => {
                self.theme = config::load(&self.dir, "theme.json");
                self.say("§aSpotify HUD §8» §7Colours reloaded.");
                return;
            }
            Item::ClientId => {
                for l in CLIENT_ID_HELP {
                    self.say(*l);
                }
                return;
            }
            Item::Login => {
                self.service.send(Action::Login);
                return;
            }
            Item::Logout => {
                self.service.send(Action::Logout);
                return;
            }
            Item::Restart => {
                self.restart();
                return;
            }
            Item::Volume => {
                let s = self.volume_screen();
                self.open(s);
                return;
            }
        }
        self.save();
    }

    fn restart(&mut self) {
        if self.config.music_service == "spotify" {
            self.service.send(Action::Restart);
        } else {
            self.say("§aSpotify HUD §8» §7Turn it on first (General → Enabled).");
        }
    }

    /// The labels of the mod's page in Options > Mods.
    pub fn page_labels(&self) -> Vec<String> {
        PAGE_ITEMS
            .iter()
            .map(|i| {
                let (name, value, _) = self.item_text(*i);
                format!("{name}: {value}")
            })
            .collect()
    }

    pub fn page_click(&mut self, button: usize) {
        if let Some(item) = PAGE_ITEMS.get(button) {
            self.click_item(*item, false);
        }
    }

    // ---- commands ----

    /// A command typed in chat, without its slash; None when it isn't its.
    pub fn command(&mut self, line: &str) -> Option<()> {
        let mut words = line.split_whitespace();
        if words.next()? != COMMAND {
            return None;
        }
        let sub = words.next().unwrap_or("");
        let rest = words.collect::<Vec<_>>().join(" ");
        match sub {
            "" => self.open(Screen::Settings { tab: 0 }),
            "theme" if rest == "reload" => self.click_item(Item::ReloadTheme, false),
            "theme" => self.open(Screen::Settings { tab: 2 }),
            "position" => self.open(Screen::Position { grab: None }),
            "volume" if rest.is_empty() => {
                let s = self.volume_screen();
                self.open(s);
            }
            "volume" => match rest.trim_end_matches('%').parse::<u8>() {
                Ok(v) if v <= 100 => self.service.send(Action::Volume(v)),
                _ => self.say("§cSpotify HUD §8» §7Volume goes from 0 to 100."),
            },
            "restart" => self.restart(),
            "login" => self.service.send(Action::Login),
            "logout" => self.service.send(Action::Logout),
            "clientid" if !rest.is_empty() => {
                let id = rest.trim().to_owned();
                if id.len() < 16 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    self.say("§cSpotify HUD §8» §7A Client ID is 32 letters and digits; check what you pasted.");
                } else {
                    let auth = {
                        let mut s = self.service.lock();
                        if s.auth.client_id != id {
                            s.auth.auth.clear();
                            s.auth.refresh.clear();
                        }
                        s.auth.client_id = id;
                        s.auth.clone()
                    };
                    config::save(&self.dir, "spotify.json", &auth);
                    self.say("§aSpotify HUD §8» §7Saved. Next: §9/music login§7.");
                }
            }
            "clientid" => self.click_item(Item::ClientId, false),
            "announcement" if !rest.is_empty() => {
                self.config.announcement_message = rest;
                self.save();
                self.say("§aSpotify HUD §8» §7Chat line saved.");
            }
            "play" => self.service.send(Action::SetPaused(false)),
            "pause" => self.service.send(Action::SetPaused(true)),
            "next" | "skip" => self.service.send(Action::Move(true)),
            "previous" | "prev" => self.service.send(Action::Move(false)),
            "shuffle" => self.service.send(Action::ToggleShuffle),
            "repeat" => self.service.send(Action::ToggleRepeat),
            "hide" | "toggle" => self.temporarily_hidden = !self.temporarily_hidden,
            _ => {
                self.say("§aSpotify HUD §8» §7/music [theme|position|volume [0-100]|restart|login|logout|clientid <id>|announcement <text>|play|pause|next|previous|shuffle|repeat|hide]");
            }
        }
        Some(())
    }

    // ---- its screens ----

    /// Draws the mod's open screen (`screen_render_post` while it is the mod's screen).
    pub fn screen_draw(&mut self, v: &View<'_>, width: Width<'_>) -> Vec<Cmd> {
        self.last_gui = v.gui;
        match self.screen.clone() {
            Some(Screen::Settings { tab }) => self.settings_draw(v, tab, width),
            Some(Screen::Volume { volume, .. }) => volume_draw(v, volume, &self.theme, width),
            Some(Screen::Position { .. }) => {
                let mut cmds = self.player(v, Some(v.mouse), width);
                let mut c = Canvas::default();
                let hint = "Drag the box, then press Esc.";
                let w = width(hint);
                c.text(hint, [((v.gui[0] - w) / 2.0).floor(), v.gui[1] - 20.0], 0xFFFF_FFFF, true, 1.0, None);
                cmds.extend(c.cmds);
                cmds
            }
            None => Vec::new(),
        }
    }

    fn settings_panel(v: &View<'_>, rows: usize) -> [f32; 4] {
        let w = 300f32.min(v.gui[0] - 10.0);
        let h = (44.0 + rows as f32 * 18.0 + 22.0).min(v.gui[1] - 10.0);
        [((v.gui[0] - w) / 2.0).floor(), ((v.gui[1] - h) / 2.0).floor(), w, h]
    }

    fn settings_layout(&self, v: &View<'_>, tab: usize) -> (Vec<(usize, [f32; 4])>, Vec<(Item, [f32; 4], [f32; 4])>, [f32; 4]) {
        let items = tab_items(tab);
        let p = Self::settings_panel(v, tab_items(2).len());
        let tw = p[2] / TABS.len() as f32;
        let tabs = (0..TABS.len()).map(|i| (i, [p[0] + i as f32 * tw, p[1] + 18.0, tw, 16.0])).collect();
        let rows = items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let y = p[1] + 40.0 + i as f32 * 18.0;
                let row = [p[0] + 6.0, y, p[2] - 12.0, 16.0];
                let button = [p[0] + p[2] - 6.0 - 130.0, y, 130.0, 16.0];
                (item, row, button)
            })
            .collect();
        (tabs, rows, p)
    }

    fn settings_draw(&self, v: &View<'_>, tab: usize, width: Width<'_>) -> Vec<Cmd> {
        let t = &self.theme;
        let (tabs, rows, p) = self.settings_layout(v, tab);
        let mut c = Canvas::default();
        c.bordered(p, t.background_radius, 0xD010_1010, t.border_color.0, 1.0);
        let title = format!("{NAME} {VERSION}");
        c.text(&title, [p[0] + ((p[2] - width(&title)) / 2.0).floor(), p[1] + 6.0], t.border_color.0, true, 1.0, None);
        for (i, r) in tabs {
            let over = inside(v.mouse, r);
            let (fill, ink) = if i == tab {
                (0x40FF_FFFF, 0xFFFF_FFFF)
            } else if over {
                (0x20FF_FFFF, 0xFFDD_DDDD)
            } else {
                (0, 0xFFAA_AAAA)
            };
            c.fill(r, fill);
            if i == tab {
                c.fill([r[0], r[1] + r[3] - 1.0, r[2], 1.0], t.border_color.0);
            }
            c.text_in(width, TABS[i], r, 0.5, ink, 1.0, None);
        }
        let mut hint = "Left click: next value · Right click: previous · Esc: close";
        for (item, row, button) in rows {
            let (name, value, desc) = self.item_text(item);
            c.text_in(width, &name, [row[0], row[1], row[2] - button[2] - 4.0, row[3]], 0.0, 0xFFFF_FFFF, 1.0, None);
            let over = inside(v.mouse, button);
            c.bordered(button, 2.0, if over { 0x60FF_FFFF } else { 0x40FF_FFFF }, if over { t.border_color.0 } else { 0x40FF_FFFF }, 1.0);
            let colour = match value.as_str() {
                "ON" => t.selected_control_color.0,
                "OFF" => 0xFFFF_5555,
                _ => 0xFFFF_FFFF,
            };
            c.text_in(width, &value, button, 0.5, colour, 1.0, Some(self.ticks));
            if inside(v.mouse, row) {
                hint = desc;
            }
        }
        // The hovered setting's description, small, at the bottom.
        let fw = (p[2] - 12.0) / 0.5;
        let mut line = String::new();
        let mut lines = Vec::new();
        for word in hint.split(' ') {
            let next = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if width(&next) > fw && !line.is_empty() {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = next;
            }
        }
        lines.push(line);
        for (i, l) in lines.iter().take(2).enumerate() {
            c.text(l, [p[0] + 6.0, p[1] + p[3] - 16.0 + i as f32 * 5.0], 0xFFAA_AAAA, false, 0.5, None);
        }
        c.cmds
    }

    fn screen_press(&mut self, v: &View<'_>, button: i32, _width: Width<'_>) -> bool {
        let back = button == 1;
        match self.screen.clone() {
            Some(Screen::Settings { tab }) => {
                let (tabs, rows, _) = self.settings_layout(v, tab);
                if let Some((i, _)) = tabs.iter().find(|(_, r)| inside(v.mouse, *r)) {
                    self.screen = Some(Screen::Settings { tab: *i });
                    return true;
                }
                if let Some((item, ..)) = rows.iter().find(|(_, _, b)| inside(v.mouse, *b)) {
                    self.click_item(*item, back);
                }
                true
            }
            Some(Screen::Volume { volume, last, .. }) => {
                let icon = volume_icon_rect(v);
                if inside(v.mouse, icon) {
                    let (volume, last) = if volume > 0.0 { (0.0, volume) } else { (last, last) };
                    self.screen = Some(Screen::Volume { volume, last, dragging: false });
                    self.service.send(Action::Volume((volume * 100.0) as u8));
                    return true;
                }
                let (bx, bw) = volume_bar(v);
                let [_, py, _, _] = volume_panel(v);
                if inside(v.mouse, [bx - 4.0, py + 18.0, bw + 8.0, 7.0]) {
                    let volume = ((v.mouse[0] - bx) / bw).clamp(0.0, 1.0);
                    self.screen = Some(Screen::Volume { volume, last, dragging: true });
                }
                true
            }
            Some(Screen::Position { .. }) => {
                let [x, y, w, h] = self.drawn.rect;
                let (dx, dy) = (v.mouse[0] - x, v.mouse[1] - y);
                let grab = (dx >= 0.0 && dx <= w && dy >= 0.0 && dy <= h).then_some([dx, dy]);
                self.screen = Some(Screen::Position { grab });
                true
            }
            None => false,
        }
    }
}

fn volume_panel(v: &View<'_>) -> [f32; 4] {
    [((v.gui[0] - 140.0) / 2.0).floor(), ((v.gui[1] - 30.0) / 2.0).floor(), 140.0, 30.0]
}

fn volume_icon_rect(v: &View<'_>) -> [f32; 4] {
    let p = volume_panel(v);
    [p[0] + 5.0, p[1] + 15.0, 12.0, 12.0]
}

/// The slider's track: its left edge and width.
fn volume_bar(v: &View<'_>) -> (f32, f32) {
    let p = volume_panel(v);
    let h = 7.0;
    (p[0] + 22.0 + 1.0 + h * 0.75, 90.0 - 2.0 - h * 1.5)
}

/// its volume screen: "Set Volume", the speaker (a click mutes), the slider and the
/// percentage.
fn volume_draw(v: &View<'_>, volume: f32, t: &Theme, width: Width<'_>) -> Vec<Cmd> {
    let mut c = Canvas::default();
    let p = volume_panel(v);
    c.fill(p, 0x5000_0000);
    c.text_in(width, "Volume", [p[0], p[1] + 5.0 - 0.5, p[2], 9.0], 0.5, 0xFFFF_FFFF, 1.0, None);
    let icon = volume_icon_rect(v);
    let colour = if inside(v.mouse, icon) { 0xFF80_8080 } else { 0xFFFF_FFFF };
    c.icon(if volume > 0.0 { &draw::ICON_VOLUME } else { &draw::ICON_MUTED }, [icon[0], icon[1]], 12.0, colour);
    let (bx, bw) = volume_bar(v);
    let sy = p[1] + 18.0;
    c.fill([bx, sy + 1.75, bw, 3.5], 0xFF22_2222);
    c.fill([bx, sy + 1.75, bw * volume, 3.5], t.selected_control_color.0);
    c.fill([bx + bw * volume - 3.5, sy, 7.0, 7.0], 0xFFFF_FFFF);
    c.text(&format!("{}%", (volume * 100.0) as u32), [p[0] + 117.0, p[1] + 19.0], 0xFFFF_FFFF, true, 0.5, None);
    c.cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("spotify-hud-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn view(screen: Option<&str>) -> View<'_> {
        View { gui: [427.0, 240.0], scale: 2.0, mouse: [10.0, 10.0], screen, debug: false, gui_hidden: false, in_world: true }
    }

    #[test]
    fn hidden_until_a_song_and_spotify() {
        let mut c = SpotifyHud::new(temp_dir("hidden"));
        let w = |s: &str| s.len() as f32 * 6.0;
        assert!(c.hud(&view(None), false, &w).is_empty(), "disabled by default");
        c.config.music_service = "spotify".into();
        assert!(c.hud(&view(None), false, &w).is_empty(), "no song yet");
        c.config.display_mode = DisplayMode::AllTheTime;
        assert!(!c.hud(&view(None), false, &w).is_empty());
        assert!(c.hud(&view(Some("inventory")), false, &w).is_empty(), "non intrusive");
    }

    #[test]
    fn hovering_in_chat_shows_the_controls() {
        let mut c = SpotifyHud::new(temp_dir("hover"));
        let w = |s: &str| s.len() as f32 * 6.0;
        c.config.music_service = "spotify".into();
        c.config.display_mode = DisplayMode::AllTheTime;
        c.hud(&view(Some("chat")), false, &w);
        assert_eq!(c.drawn.rect, [0.0, 0.0, 150.0, 63.0]);
        assert_eq!(c.drawn.buttons.len(), 9);
        let (_, first) = c.drawn.buttons[0];
        assert_eq!(first, [5.0 + ((140.0f32 - 106.0) / 2.0).floor(), 49.0, 10.0, 10.0]);
        // Settings opens its screen.
        let mut v = view(Some("chat"));
        let settings = c.drawn.buttons[1].1;
        v.mouse = [settings[0] + 2.0, settings[1] + 2.0];
        assert!(c.press(&v, false, 0, &w));
        assert_eq!(c.opening.as_ref().map(|o| o.0.clone()), Some(Screen::Settings { tab: 0 }));
    }

    #[test]
    fn commands_are_its_own_only() {
        let mut c = SpotifyHud::new(temp_dir("cmd"));
        assert!(c.command("gamemode creative").is_none());
        assert!(c.command("music volume").is_some());
        assert!(matches!(c.opening, Some((Screen::Volume { .. }, 0))));
        c.command("music clientid 0123456789abcdef0123456789abcdef");
        assert_eq!(c.service.lock().auth.client_id, "0123456789abcdef0123456789abcdef");
        c.command("music announcement Now: ${song}");
        assert_eq!(c.config.announcement_message, "Now: ${song}");
        let saved: Config = config::load(&c.dir, "config.json");
        assert_eq!(saved.announcement_message, "Now: ${song}");
    }

    #[test]
    fn settings_cycle_and_persist() {
        let mut c = SpotifyHud::new(temp_dir("settings"));
        let labels = c.page_labels();
        assert_eq!(labels[0], "Enabled: Off");
        c.page_click(3);
        assert_eq!(c.config.anchor_point, Anchor::TopMiddle);
        c.click_item(Item::Show(Control::Volume), false);
        assert!(!c.theme.show_volume_button);
        let theme: Theme = config::load(&c.dir, "theme.json");
        assert!(!theme.show_volume_button);
    }

    #[test]
    fn times_format_as_mm_ss() {
        assert_eq!(format_time(61_500.0), "01:01");
        assert_eq!(format_time(0.0), "00:00");
    }
}
