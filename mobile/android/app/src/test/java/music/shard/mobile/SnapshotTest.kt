package music.shard.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SnapshotTest {
    @Test fun playlistOrderAndGlobalFavoritesSurviveProjection() {
        val snapshot = Snapshot(JSONObject("""{
          "tracks":[
            {"id":3,"title":"Third","artist":"Artist","file_path":"audio/3.flac","duration_ms":3000},
            {"id":1,"title":"First","artist":"Artist","file_path":"audio/1.m4a","duration_ms":1000},
            {"id":2,"title":"Second","artist":"Artist","file_path":"audio/2.opus","duration_ms":null}],
          "playlists":[{"id":7,"name":"Mix","track_count":2,"cover_path":null,"cover_color":null,"header_color":null}],
          "members":{"7":[2,1]},"favorites":[1,3],"backgrounds":[],"preferences":{},"statistics":{}
        }"""))
        assertEquals(listOf(2L, 1L), snapshot.tracksIn(7).map { it.id })
        assertEquals(listOf(3L, 1L), snapshot.tracksIn(-1).map { it.id })
        assertEquals(listOf(3L, 1L, 2L), snapshot.tracksIn(null).map { it.id })
        assertNull(snapshot.playlists.single().cover)
        assertNull(snapshot.playlists.single().color)
        assertEquals(0L, snapshot.tracks.last().duration)
        assertTrue(snapshot.tracksIn(999).isEmpty())
    }
}
