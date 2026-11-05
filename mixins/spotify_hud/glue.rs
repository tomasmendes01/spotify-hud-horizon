// The game glue (`crate::game::spotify_hud_glue`): the view the engine reads, and
// its shapes drawn with Horizon's HUD helpers. Everything else is in the `spotify-hud-engine` crate.

use spotify_hud_engine::draw::Cmd;

pub(crate) const ID: &str = spotify_hud_engine::ID;

/// The frame as the mod sees it: the GUI's size and scale, the pointer, the open screen,
/// F3, and whether the player is in a world.
pub(crate) fn view<'a>(
    game: &crate::game::Game<'_>,
    gui: [f32; 2],
    scale: f32,
    mouse: [f32; 2],
    screen: Option<&'a str>,
) -> spotify_hud_engine::View<'a> {
    spotify_hud_engine::View {
        gui,
        scale,
        mouse,
        screen,
        debug: !game.native_corner_shown(horizon_client::extras::Corner::TopLeft),
        gui_hidden: false,
        in_world: game.native_player().is_some(),
    }
}

/// Whether the original's own screen is the open one.
pub(crate) fn own_screen(game: &crate::game::Game<'_>) -> bool {
    game.native_mod_screen() == Some(ID)
}

/// Draws the engine's shapes: fills, and text scaled about its top left and cut to its box
/// through the HUD batch.
pub(crate) fn paint(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, cmds: &[Cmd]) {
    for cmd in cmds {
        match cmd {
            Cmd::Fill(rect, argb) => game.native_fill(batch, *rect, *argb),
            Cmd::Text { text, at, rgb, shadow, scale, clip } => {
                let start = batch.vertices.len();
                game.native_text(batch, text, *at, *rgb, *shadow);
                if (*scale - 1.0).abs() > f32::EPSILON {
                    batch.scale_since(start, at[0], at[1], *scale, *scale);
                }
                if let Some([x0, y0, x1, y1]) = clip {
                    batch.clip_since(start, *x0, *y0, *x1, *y1);
                }
            }
        }
    }
}
