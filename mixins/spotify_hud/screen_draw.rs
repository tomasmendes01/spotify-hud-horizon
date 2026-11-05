// Every open screen: the mod notes which it is (the box's Render Type reads it) and draws its
// own screens: settings, volume and the position editor.
fn spotify_hud_screen_draw(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::mod_api::NativeScreenView<'_>,
) {
    use crate::game::spotify_hud_glue as glue;
    let Some(mut c) = spotify_hud_engine::started() else { return };
    c.saw_screen(screen.id);
    if !glue::own_screen(game) {
        return;
    }
    let view = glue::view(game, [batch.width, batch.height], batch.scale, screen.mouse, Some("mod"));
    let cmds = c.screen_draw(&view, &|t: &str| game.native_text_width(t));
    drop(c);
    glue::paint(game, batch, &cmds);
}
