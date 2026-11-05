// /music and its subcommands, run on the client.
fn spotify_hud_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::spotify_hud_glue as glue;
    if *ran || !game.native_on(glue::ID) {
        return;
    }
    let dir = game.native_data_dir(glue::ID);
    let mut c = spotify_hud_engine::get(|| dir);
    if c.command(command).is_some() {
        *ran = true;
    }
}
