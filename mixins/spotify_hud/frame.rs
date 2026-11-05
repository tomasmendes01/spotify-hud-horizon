// Once a frame: the marquee's clock and the tab list key, and its screen opened once
// the game lets it (a click in chat closes chat first).
fn spotify_hud_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    use crate::game::spotify_hud_glue as glue;
    if !game.native_window() || !game.native_on(glue::ID) {
        return;
    }
    let Some(mut c) = spotify_hud_engine::started() else { return };
    let tab = game.native_playing() && game.native_mapping("key.playerlist").is_some_and(|m| m.0);
    c.frame(elapsed.as_secs_f32(), tab, glue::own_screen(game));
    let Some(screen) = c.take_opening() else { return };
    drop(c);
    if game.native_player().is_none() {
        return;
    }
    if game.screen == crate::game::Screen::Chat {
        game.close_chat(crate::game::chat::ChatExit::Interrupted);
    }
    if game.native_mod_screen_open(glue::ID) {
        if let Some(mut c) = spotify_hud_engine::started() {
            c.opened(screen);
        }
    }
}
