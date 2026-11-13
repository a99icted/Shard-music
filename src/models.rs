#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub source_url: Option<String>,
    pub file_path: String,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub track_count: i64,
    pub duration_ms: i64,
    pub cover_path: Option<String>,
    pub cover_color: Option<u32>,
    pub header_color: Option<u32>,
}

pub struct NewTrack {
    pub title: String,
    pub artist: String,
    pub source_url: Option<String>,
    pub file_path: String,
    pub duration_ms: Option<i64>,
}
