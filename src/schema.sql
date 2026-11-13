CREATE TABLE IF NOT EXISTS tracks (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    artist TEXT NOT NULL DEFAULT '',
    source_url TEXT,
    file_path TEXT NOT NULL UNIQUE,
    duration_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS playlists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (playlist_id, track_id),
    UNIQUE (playlist_id, position)
);

CREATE INDEX IF NOT EXISTS playlist_tracks_track_id ON playlist_tracks(track_id);

CREATE TABLE IF NOT EXISTS playlist_appearance (
    playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE,
    cover_path TEXT,
    cover_color INTEGER CHECK (cover_color BETWEEN 0 AND 16777215),
    header_color INTEGER CHECK (header_color BETWEEN 0 AND 16777215)
);

CREATE TABLE IF NOT EXISTS listening_days (
    day TEXT PRIMARY KEY,
    listened_ms INTEGER NOT NULL CHECK (listened_ms >= 0)
);

CREATE TABLE IF NOT EXISTS listening_sessions (
    id TEXT PRIMARY KEY,
    track_id INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
    listened_ms INTEGER NOT NULL CHECK (listened_ms >= 0)
);
CREATE INDEX IF NOT EXISTS listening_sessions_track ON listening_sessions(track_id);

CREATE TABLE IF NOT EXISTS backgrounds (
    scope_id INTEGER PRIMARY KEY CHECK (scope_id >= 0),
    playlist_id INTEGER UNIQUE REFERENCES playlists(id) ON DELETE CASCADE,
    original_path TEXT NOT NULL,
    display_path TEXT NOT NULL,
    blur INTEGER NOT NULL CHECK (blur BETWEEN 0 AND 40),
    CHECK ((scope_id = 0 AND playlist_id IS NULL) OR
           (scope_id > 0 AND playlist_id IS NOT NULL AND scope_id = playlist_id))
);
