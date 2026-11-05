# Spotify HUD

A [Horizon](https://playhorizon.dev) native mod that shows the song Spotify is playing in a box on
the HUD: cover, title, artists and progress. With chat open, point at the box to get buttons
for play/pause, next, previous, shuffle, repeat, volume and the song's link.

Made by tomasmendes01 (in-game name: **iLoong**). Inspired by ThatGravyBoat's
[Craftify](https://github.com/ThatGravyBoat/Craftify), written from scratch for Horizon; not
affiliated with ThatGravyBoat, Spotify, Mojang or Microsoft. The polling model of its Spotify
service follows ThatGravyBoat's [Jukebox](https://github.com/ThatGravyBoat/Jukebox) (MIT).

## Connect Spotify (one time)

The mod logs in with Spotify's PKCE login, through a Spotify app of your own (it's free):

1. Go to <https://developer.spotify.com/dashboard> and press **Create app**.
2. Add the redirect URI `http://127.0.0.1:21851/spotify/callback`, tick **Web API**, save.
3. In a world, paste the app's Client ID: `/music clientid <Client ID>`.
4. Run `/music login` and allow it in the browser tab that opens (within two minutes).

The buttons need **Spotify Premium** (Spotify refuses them from free accounts) and a song playing
on some device.

## Using it

- `/music` opens the settings: **General** (on/off, song link, now-playing chat line),
  **Display** (corner, screens, when, buttons, scroll speed, position), **Look** (which buttons,
  colours) and **Account** (Spotify app, connect, disconnect, restart, volume).
- `/music theme | position | volume [0-100] | restart | login | logout | clientid <id> |
  announcement <text> | play | pause | next | previous | shuffle | repeat | hide`
- Keys, unbound until you pick them in Options → Mods → Spotify HUD: Next song, Previous song,
  Play / pause, Show / hide the box.
- Options → Mods → Spotify HUD has the main settings as buttons.

Everything is kept in `horizon-mods/.horizon-mod-data/spotify_hud/`: `config.json`, `spotify.json`
(the login) and `theme.json`, whose colours (`#AARRGGBB`) and radii you can edit and reload from
Look → Colours.

Horizon mods can't load images, so the cover is drawn cell by cell, one cell per screen pixel.

## Building

A Horizon native mod bundle: `horizon-mixin.toml`, small helpers on Horizon's native hook points,
and the `spotify-hud-engine` crate in `engine/` (its dependencies are all in Horizon's
`Cargo.lock`). Install `spotify_hud.hnmod` from the Mods page, or put it in the instance's
`horizon-native-mods/`, turn it on and press **Build**; or
`horizon-launcher native-build mixins/spotify_hud --trust-native-code`.

## Licence

MIT, see [LICENSE](LICENSE).
