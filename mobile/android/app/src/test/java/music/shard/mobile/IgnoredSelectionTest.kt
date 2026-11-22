package music.shard.mobile

import org.junit.Assert.*
import org.junit.Test

class IgnoredSelectionTest {
    private val tracks = (1L..4L).map { Track(it, "Track $it", "Artist", "audio/$it.wav", 1000) }
    @Test fun explicitSelectionAllowsOnlyTheSelectedIgnoredTrack() {
        assertEquals(listOf(1L, 2L, 4L), playbackSelection(tracks, setOf(2L, 3L), 2L).map { it.id })
        assertEquals(listOf(1L, 4L), playbackSelection(tracks, setOf(2L, 3L), 4L).map { it.id })
        assertEquals(listOf(3L), playbackSelection(tracks, tracks.map { it.id }.toSet(), 3L).map { it.id })
    }
}
