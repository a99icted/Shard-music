# Spotify metadata import

Public playlists and albums use the anonymous web-player queries `fetchPlaylist`
and `getAlbum`, following the approach used by spotDL's SpotipyFree/SpotAPI
backend. The implementation is native Rust; Python and spotDL are not runtime
dependencies. Individual tracks continue to use Spotify's embed metadata.

`src/spotify/web.rs` discovers the web-player bundle from Spotify's public HTML,
reads the current persisted query hash, obtains an anonymous access token, and
requests pages of 100 items. The TOTP version data comes from the public
ThetaDev spotify-secrets endpoint also used by SpotAPI. These are internal
web-player interfaces and may need maintenance when Spotify changes them.
Failure to fetch a page or an inconsistent total produces an error instead of
silently falling back to the truncated embed list.

Offsets count every source item, including restricted/local items. Spotify
`RestrictedContent` entries without track metadata are omitted from the available
tracks. Source order and duplicate track entries are preserved; existing library
duplicate handling still applies during audio import.

Playlist track artwork comes from `albumOfTrack.coverArt`. The playlist image is
saved for the collection and is never used as a track fallback. A missing or
failed track image falls back to the matched YouTube thumbnail. Album imports
retain the album-cover fallback. Existing library covers are not migrated.

## Verification

```powershell
cargo test --locked --no-default-features --lib spotify
cargo test --locked --no-default-features --lib spotify::web::tests::fetches_complete_public_playlist_and_paged_albums -- --ignored --nocapture
cargo test --locked --no-default-features --lib music_metadata::tests::downloads_spotify_playlist_and_album_samples_with_distinct_artwork_rules -- --ignored --nocapture
```

The last test downloads one playlist track and one album track into temporary
directories, validates the selected artwork and decodes the image. It requires
the usual yt-dlp/FFmpeg components and access to YouTube. Album pagination is
also exercised with pages of seven tracks in the metadata network test.

Reference implementations:

- [spotDL client selection](https://github.com/spotDL/spotify-downloader/blob/58ced9af69dce17d33ff280bafbcc917a9dba0aa/spotdl/utils/spotify.py)
- [spotDL playlist enumeration](https://github.com/spotDL/spotify-downloader/blob/58ced9af69dce17d33ff280bafbcc917a9dba0aa/spotdl/types/playlist.py)
- [SpotAPI](https://github.com/Aran404/SpotAPI)
