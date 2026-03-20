package net.murmur.app

import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.util.Log
import net.murmur.app.storage.BlobStore

private const val TAG = "MurmurDocumentsProvider"

// -----------------------------------------------------------------------
// Root columns
// -----------------------------------------------------------------------
private val ROOT_PROJECTION = arrayOf(
    Root.COLUMN_ROOT_ID,
    Root.COLUMN_MIME_TYPES,
    Root.COLUMN_FLAGS,
    Root.COLUMN_ICON,
    Root.COLUMN_TITLE,
    Root.COLUMN_DOCUMENT_ID,
    Root.COLUMN_AVAILABLE_BYTES
)

// -----------------------------------------------------------------------
// Document columns
// -----------------------------------------------------------------------
private val DOCUMENT_PROJECTION = arrayOf(
    Document.COLUMN_DOCUMENT_ID,
    Document.COLUMN_MIME_TYPE,
    Document.COLUMN_DISPLAY_NAME,
    Document.COLUMN_LAST_MODIFIED,
    Document.COLUMN_FLAGS,
    Document.COLUMN_SIZE
)

private const val ROOT_ID = "murmur_root"
private const val ROOT_DOC_ID = "murmur/"

/**
 * Exposes Murmur-synced files in the Android Files app via the
 * [DocumentsProvider] API.
 *
 * - [queryRoots]: returns one root labelled "Murmur"
 * - [queryChildDocuments]: lists blobs stored in [BlobStore]
 * - [openDocument]: returns a [ParcelFileDescriptor] for the requested blob
 */
class MurmurDocumentsProvider : DocumentsProvider() {

    private lateinit var blobStore: BlobStore

    override fun onCreate(): Boolean {
        blobStore = BlobStore(context!!)
        return true
    }

    override fun queryRoots(projection: Array<out String>?): Cursor {
        val result = MatrixCursor(projection ?: ROOT_PROJECTION)
        result.newRow().apply {
            add(Root.COLUMN_ROOT_ID, ROOT_ID)
            add(Root.COLUMN_TITLE, "Murmur")
            add(Root.COLUMN_DOCUMENT_ID, ROOT_DOC_ID)
            add(Root.COLUMN_MIME_TYPES, "*/*")
            add(Root.COLUMN_FLAGS, Root.FLAG_SUPPORTS_IS_CHILD)
            add(Root.COLUMN_ICON, android.R.drawable.ic_dialog_info)
        }
        return result
    }

    override fun queryDocument(
        documentId: String,
        projection: Array<out String>?
    ): Cursor {
        val result = MatrixCursor(projection ?: DOCUMENT_PROJECTION)
        if (documentId == ROOT_DOC_ID) {
            result.newRow().apply {
                add(Document.COLUMN_DOCUMENT_ID, ROOT_DOC_ID)
                add(Document.COLUMN_DISPLAY_NAME, "Murmur")
                add(Document.COLUMN_MIME_TYPE, Document.MIME_TYPE_DIR)
                add(Document.COLUMN_FLAGS, 0)
                add(Document.COLUMN_SIZE, null)
            }
        }
        return result
    }

    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?
    ): Cursor {
        val result = MatrixCursor(projection ?: DOCUMENT_PROJECTION)
        if (parentDocumentId != ROOT_DOC_ID) return result

        // List all blobs in local storage.
        // In a full implementation, we'd query the engine's file index to get
        // names and metadata; here we enumerate blobs from BlobStore.
        val blobDir = java.io.File(context!!.filesDir, "blobs")
        if (!blobDir.exists()) return result

        blobDir.walkTopDown()
            .filter { it.isFile }
            .forEach { file ->
                val docId = "blob/${file.relativeTo(blobDir).path}"
                result.newRow().apply {
                    add(Document.COLUMN_DOCUMENT_ID, docId)
                    add(Document.COLUMN_DISPLAY_NAME, file.name)
                    add(Document.COLUMN_MIME_TYPE, "application/octet-stream")
                    add(Document.COLUMN_LAST_MODIFIED, file.lastModified())
                    add(Document.COLUMN_FLAGS, 0)
                    add(Document.COLUMN_SIZE, file.length())
                }
            }
        return result
    }

    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?
    ): ParcelFileDescriptor {
        Log.d(TAG, "openDocument: $documentId mode=$mode")

        val blobDir = java.io.File(context!!.filesDir, "blobs")
        val relativePath = documentId.removePrefix("blob/")
        val file = java.io.File(blobDir, relativePath)

        if (!file.exists()) {
            throw java.io.FileNotFoundException("Blob not found: $documentId")
        }

        val accessMode = ParcelFileDescriptor.parseMode(mode)
        return ParcelFileDescriptor.open(file, accessMode)
    }
}
