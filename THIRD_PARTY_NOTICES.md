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

## Android application

`mobile/android` resolves AndroidX Compose, Media3, WorkManager (Apache-2.0),
Coil (Apache-2.0), JNI bindings (MIT / Apache-2.0), and their transitive
dependencies through Gradle and Cargo. The Android downloader uses
`io.github.junkfood02.youtubedl-android:library:0.18.1` and `ffmpeg:0.18.1`.
Its artifacts contain native Python, yt-dlp, QuickJS and FFmpeg components;
these retain their own licenses and are not covered by Shard's MIT License.

- Android wrapper and build sources: https://github.com/yausername/youtubedl-android
- Python license: https://docs.python.org/3/license.html
- QuickJS: https://bellard.org/quickjs/ (MIT)
- FFmpeg build instructions: https://github.com/yausername/youtubedl-android/blob/master/BUILD_FFMPEG.md
- Gradle Wrapper and Gradle: https://github.com/gradle/gradle (Apache-2.0)

Preserve notices and corresponding-source obligations of the exact Android
native artifacts when redistributing an APK. Debug APKs and third-party native
binaries are generated build outputs and are excluded from this repository.

## Windows installer payload

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
