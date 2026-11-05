// its row in Options > Mods, with its keys.
fn spotify_hud_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    let keys = spotify_hud_engine::KEYS
        .iter()
        .map(|(name, label)| [(*name).to_owned(), (*label).to_owned(), spotify_hud_engine::UNBOUND.to_owned()])
        .collect();
    mods.push(crate::game::native_hooks::NativeMod {
        id: spotify_hud_engine::ID.into(),
        title: format!("{} {}", spotify_hud_engine::NAME, spotify_hud_engine::VERSION),
        description: spotify_hud_engine::DESCRIPTION.into(),
        keys,
    });
}
