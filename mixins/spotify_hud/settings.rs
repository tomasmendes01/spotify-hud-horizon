// its settings page in Options > Mods: its settings as buttons (the theme is in
// /music's screen and theme.json).
fn spotify_hud_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != spotify_hud_engine::ID {
        return;
    }
    let dir = game.native_data_dir(spotify_hud_engine::ID);
    buttons.extend(spotify_hud_engine::get(|| dir).page_labels());
}
