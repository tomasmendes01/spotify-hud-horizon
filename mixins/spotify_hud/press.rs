// A click over a screen: the player box's controls while chat (or another screen) is open, and
// the original's own screens.
fn spotify_hud_press(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    use crate::game::spotify_hud_glue as glue;
    let Some(mut c) = spotify_hud_engine::started() else { return };
    let gui = c.last_gui;
    let view = glue::view(game, gui, 1.0, screen.mouse, Some(screen.id));
    if c.press(&view, glue::own_screen(game), button, &|t: &str| game.native_text_width(t)) {
        *canceled = true;
    }
}
