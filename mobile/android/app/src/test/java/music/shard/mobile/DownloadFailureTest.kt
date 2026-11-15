package music.shard.mobile

import org.junit.Assert.*
import org.junit.Test

class DownloadFailureTest {
    @Test fun oldVersionWarningDoesNotHideActual403() {
        val raw = "WARNING: Your yt-dlp version (2025.11.12) is older than 90 days!\n" +
            "WARNING: YouTube is forcing SABR streaming for this client.\n" +
            "ERROR: unable to download video data: HTTP Error 403: Forbidden"
        assertTrue(DownloadFailure.needsExtractorRefresh(raw))
        assertTrue(DownloadFailure.summary(raw, false).contains("403"))
        assertFalse(DownloadFailure.summary(raw, false).contains("90 days"))
    }

    @Test fun accountRestrictionsHaveUsefulMessage() {
        assertTrue(DownloadFailure.summary("ERROR: Sign in to confirm your age", false).contains("вход"))
        assertFalse(DownloadFailure.needsExtractorRefresh("ERROR: Private video"))
    }

    @Test fun detailsFitWorkManagerEvenForCyrillicAndEmoji() {
        val report = "Ошибка загрузки 🎵\n".repeat(3000)
        val excerpt = utf8Excerpt(report)
        assertTrue(excerpt.toByteArray(Charsets.UTF_8).size <= 6004)
        assertTrue(excerpt.endsWith("\n…"))
        assertEquals(excerpt, excerpt.toByteArray(Charsets.UTF_8).toString(Charsets.UTF_8))
        assertEquals("Короткий отчёт", utf8Excerpt("Короткий отчёт"))
    }
}
