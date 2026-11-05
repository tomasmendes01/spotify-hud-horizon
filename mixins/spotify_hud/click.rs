// A click on its settings page.
fn spotify_hud_page_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id != spotify_hud_engine::ID {
        return;
    }
    let dir = game.native_data_dir(spotify_hud_engine::ID);
    spotify_hud_engine::get(|| dir).page_click(button);
}
