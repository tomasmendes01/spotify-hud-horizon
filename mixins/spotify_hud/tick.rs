// Once a tick in the window's game: the mod starts with its folder, and its chat lines
// (logins, the first-time message, announcements) go to chat.
fn spotify_hud_tick(game: &mut crate::game::Game<'_>) {
    use crate::game::spotify_hud_glue as glue;
    if !game.native_window() || !game.native_on(glue::ID) {
        return;
    }
    let lines = {
        let dir = game.native_data_dir(glue::ID);
        let mut c = spotify_hud_engine::get(|| dir);
        c.tick(game.native_player().is_some())
    };
    for line in lines {
        game.native_chat(&line);
    }
}
