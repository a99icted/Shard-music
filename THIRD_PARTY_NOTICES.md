# Third-party notices

The original Shard code is licensed under the MIT License in `LICENSE`.
The following included or adapted code retains its original license.

## GPUI 0.2.2

- Upstream: https://github.com/zed-industries/zed
- Copyright 2022 - 2025 Zed Industries, Inc.
- License: Apache License 2.0, preserved in `vendor/gpui/LICENSE-APACHE`.
- Included sources: `vendor/gpui/`.
- Local modifications are described in `vendor/gpui/SHARD-PATCH.md`.

`src/input.rs` adapts text editing code from GPUI 0.2.2
`examples/input.rs`. It has been modified for Shard, including application
styling, themes and input behavior. It retains the Apache License 2.0;
the full license and upstream copyright are in `vendor/gpui/LICENSE-APACHE`.

## Dependencies and optional download tools

Cargo dependencies retain their respective licenses. `Cargo.lock` records
the versions used by this application.

`yt-dlp`, FFmpeg and ffprobe are separate third-party programs downloaded
by `scripts/setup-youtube.ps1`; their binaries are not included in this
source repository and are not covered by the Shard MIT License.

- yt-dlp: https://github.com/yt-dlp/yt-dlp#license
- FFmpeg: https://ffmpeg.org/legal.html
- FFmpeg builds used by the setup script: https://github.com/yt-dlp/FFmpeg-Builds

When distributing these binaries, preserve the notices and comply with the
licenses of the exact builds you distribute. In particular, the setup script
selects a GPL FFmpeg build. Publishing the Shard source repository does not
require bundling those binaries.

## Windows installer

`scripts/package-windows.ps1` creates a separate installer payload containing the
above download tools, Node.js and the Microsoft Visual C++ runtime. Their licenses
and notices are included in the installed `licenses/` directory. The exact bundled
versions and SHA-256 file hashes are recorded in `package-manifest.json`.

- Node.js: https://nodejs.org/ (MIT and bundled third-party notices).
- Visual C++ runtime: unmodified redistributable files from Visual Studio's
  `VC/Redist/MSVC` directory, distributed under Microsoft's applicable license:
  https://visualstudio.microsoft.com/license-terms/
- Inno Setup is used only to build the installer; its compiler is excluded from
  the application payload: https://jrsoftware.org/isinfo.php
