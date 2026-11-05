// The wheel over the volume screen moves the volume by 5%.
fn spotify_hud_scroll(game: &crate::game::Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    use crate::game::spotify_hud_glue as glue;
    if !glue::own_screen(game) {
        return;
    }
    let Some(mut c) = spotify_hud_engine::started() else { return };
    if c.scroll(scroll[1]) {
        *canceled = true;
    }
}
