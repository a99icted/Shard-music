package music.shard.mobile

import org.junit.Assert.*
import org.junit.Test
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

class DownloadFailureTest {
    @Test fun challengeTimeoutBefore403IsRetriedAndExplained() = runBlocking {
        val raw = "WARNING: [jsc] Failed to load player for JS challenge: The read operation timed out\n" +
            "WARNING: [SSL: UNEXPECTED_EOF_WHILE_READING] EOF occurred in violation of protocol\n" +
            "ERROR: unable to download video data: HTTP Error 403: Forbidden"
        var attempts = 0
        val result = withTransientDownloadRetry {
            if (++attempts == 1) error(raw)
            "audio"
        }
        assertEquals("audio", result)
        assertEquals(2, attempts)
        assertTrue(DownloadFailure.summary(raw, false).contains("время ожидания"))
        assertTrue(DownloadFailure.summary(raw, true).contains("timed out"))
    }

    @Test fun persistentNetworkFailureStopsAfterOneRetry() = runBlocking {
        var attempts = 0
        val failure = IllegalStateException("Connection reset by peer")
        try {
            withTransientDownloadRetry<Unit> { attempts++; throw failure }
            fail("Persistent failures must be reported")
        } catch (error: IllegalStateException) {
            assertSame(failure, error)
        }
        assertEquals(2, attempts)
    }

    @Test fun plain403AndCancellationAreNotRetried() = runBlocking {
        for (failure in listOf(IllegalStateException("HTTP Error 403: Forbidden"), CancellationException("timed out"))) {
            var attempts = 0
            try {
                withTransientDownloadRetry<Unit> { attempts++; throw failure }
                fail("Failure must propagate")
            } catch (error: Exception) {
                assertSame(failure, error)
            }
            assertEquals(1, attempts)
        }
    }

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
