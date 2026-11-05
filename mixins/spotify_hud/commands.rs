// /music in the chat box's completion, coloured as the game's own commands.
fn spotify_hud_commands(game: &mut crate::game::Game<'_>, commands: &mut Vec<&'static str>) {
    if game.native_on(crate::game::spotify_hud_glue::ID) {
        commands.push("spotify_hud");
    }
}
