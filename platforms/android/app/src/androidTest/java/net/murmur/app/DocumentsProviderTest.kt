package net.murmur.app

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import net.murmur.app.storage.BlobStore
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Instrumented tests for [MurmurDocumentsProvider].
 *
 * Tests:
 * - [testQueryRootsReturnsOneRoot] — queryRoots returns exactly one root
 * - [testQueryChildDocumentsReflectsStoredBlobs] — file count matches BlobStore
 */
@RunWith(AndroidJUnit4::class)
class DocumentsProviderTest {

    private val authority = "net.murmur.app.documents"
    private val contentResolver: ContentResolver =
        ApplicationProvider.getApplicationContext<android.app.Application>().contentResolver
    private lateinit var blobStore: BlobStore

    @Before
    fun setup() {
        blobStore = BlobStore(ApplicationProvider.getApplicationContext())
        blobStore.clear()
    }

    @After
    fun teardown() {
        blobStore.clear()
    }

    @Test
    fun testQueryRootsReturnsOneRoot() {
        val rootsUri = DocumentsContract.buildRootsUri(authority)
        val cursor = contentResolver.query(rootsUri, null, null, null, null)
        assertNotNull(cursor)
        assertEquals(1, cursor!!.count)
        cursor.close()
    }

    @Test
    fun testQueryChildDocumentsReflectsStoredBlobs() {
        // Store 3 blobs so the provider has something to list.
        repeat(3) { i ->
            val hash = ByteArray(32) { (i * 7 + it).toByte() }
            blobStore.store(hash, "content $i".toByteArray())
        }

        val rootDocUri = DocumentsContract.buildDocumentUri(authority, "murmur/")
        val childrenUri = DocumentsContract.buildChildDocumentsUri(authority, "murmur/")
        val cursor = contentResolver.query(childrenUri, null, null, null, null)
        assertNotNull(cursor)
        assertTrue("Expected at least 3 documents, got ${cursor!!.count}", cursor.count >= 3)
        cursor.close()
    }
}
