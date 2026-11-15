# Shard 0.1.4 — Smoother Window Movement & Collapsible Sidebar

This update reduces unnecessary rendering when moving the window, adds a collapsible sidebar and a Favorites heart in the player, and fixes LAN synchronization shutdown.

## What's new

- **Collapsible sidebar:** use the icon at the top to switch between the full sidebar and a compact view with the Shard logo and playlist icons. Hover over icons to see their names. Your choice is saved between sessions.
- **Favorites in the player:** add or remove the current track from Favorites using the heart beside its title and artist. The heart follows the application theme and shares its state with every playlist.

## Fixes and performance

- Removed full scene rebuilds when moving a window on the same display without changing its size. Resizing and changes to display or scaling still refresh the interface correctly.
- Reduced background statistics work while playback is idle and avoided redrawing unchanged statistics.
- Current-track lookups in the player and queue now use the library index.
- Turning off LAN synchronization now closes the listening port and active connections and cancels pending network reads and writes instead of waiting for long socket timeouts.
- Partially received audio is kept so interrupted transfers can resume after reconnecting.
- Pairing results from a disabled or restarted synchronization session no longer update the interface.

## Validation

- 68 automated tests passed; 2 tests requiring external services were ignored.
- A native window test delivered 100 movement events without rebuilding the unchanged scene, then verified that resizing still refreshed the layout.
- TCP tests cover closing the listener, cancelling stalled connections and resuming a partially received file.
- Playback pause and track switching responded in approximately 2–3 ms in a local test with a deliberately occupied SQLite writer.

## Download and update

**Windows 10 (2004+) / Windows 11, x64:** download `Shard-0.1.4-Setup-x64.exe`.

Quit Shard from the tray before updating. Your music library, playlists, Favorites and settings are preserved. Download tools and runtimes are included; no administrator rights or separate Rust or Node.js installation are required.
