// A mouse button went up over its screen: the volume slider sets the volume.
fn spotify_hud_release(
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
    if c.release(&view) {
        *canceled = true;
    }
}
