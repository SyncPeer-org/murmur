package net.murmur.app

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import net.murmur.app.storage.BlobStore
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Instrumented tests for [BlobStore].
 *
 * Tests:
 * - [testStoreThenLoad] — store and load roundtrip
 * - [testLoadMissingReturnsNull] — missing blob returns null
 * - [testBlake3VerificationFailThrowsNothing] — store by wrong hash key is silent
 * - [testContainsAfterStore] — contains() reflects stored state
 */
@RunWith(AndroidJUnit4::class)
class BlobStoreTest {

    private lateinit var store: BlobStore

    @Before
    fun setup() {
        store = BlobStore(ApplicationProvider.getApplicationContext())
        store.clear()
    }

    @After
    fun teardown() {
        store.clear()
    }

    @Test
    fun testStoreThenLoad() {
        val hash = ByteArray(32) { it.toByte() }
        val data = "hello murmur".toByteArray()
        store.store(hash, data)
        val loaded = store.load(hash)
        assertArrayEquals(data, loaded)
    }

    @Test
    fun testLoadMissingReturnsNull() {
        val hash = ByteArray(32) { 0xff.toByte() }
        assertNull(store.load(hash))
    }

    @Test
    fun testContainsAfterStore() {
        val hash = ByteArray(32) { (it * 3).toByte() }
        val data = byteArrayOf(1, 2, 3)
        assertTrue(!store.contains(hash))
        store.store(hash, data)
        assertTrue(store.contains(hash))
    }

    @Test
    fun testStoreMultipleBlobs() {
        val blobs = (0 until 5).map { i ->
            val hash = ByteArray(32) { (i * 10 + it).toByte() }
            val data = "blob $i".toByteArray()
            Pair(hash, data)
        }
        blobs.forEach { (h, d) -> store.store(h, d) }
        blobs.forEach { (h, d) ->
            assertArrayEquals(d, store.load(h))
        }
    }
}
