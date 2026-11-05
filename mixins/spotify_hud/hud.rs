// The player box on the HUD, where the mod draws it (after the HUD's own layers; F1 hides it).
fn spotify_hud_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    let _ = corners;
    use crate::game::spotify_hud_glue as glue;
    if !game.native_on(glue::ID) {
        return;
    }
    let game = &*game;
    let Some(mut c) = spotify_hud_engine::started() else { return };
    let screen = c.open_screen().map(str::to_owned);
    let view = glue::view(game, [batch.width, batch.height], batch.scale, game.native_cursor(), screen.as_deref());
    let cmds = c.hud(&view, glue::own_screen(game), &|t: &str| game.native_text_width(t));
    drop(c);
    glue::paint(game, batch, &cmds);
}
