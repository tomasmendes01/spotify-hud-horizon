// The mod's keys (Options > Mods > Spotify HUD), while playing: skip, previous, play/pause and
// hiding the box. Unbound until the player picks keys.
fn spotify_hud_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::spotify_hud_glue as glue;
    if !pressed || !game.native_playing() || !game.native_on(glue::ID) {
        return;
    }
    let game = &*game;
    let Some(mut c) = spotify_hud_engine::started() else { return };
    c.key(key, &|name: &str| game.native_mod_key(glue::ID, name, spotify_hud_engine::UNBOUND));
}
