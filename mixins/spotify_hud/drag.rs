// A drag over its screen: the volume slider, or the player in the position editor.
fn spotify_hud_drag(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    canceled: &mut bool,
) {
    use crate::game::spotify_hud_glue as glue;
    if !glue::own_screen(game) {
        return;
    }
    let Some(mut c) = spotify_hud_engine::started() else { return };
    let view = glue::view(game, c.last_gui, 1.0, screen.mouse, Some(screen.id));
    if c.drag(&view) {
        *canceled = true;
    }
}
