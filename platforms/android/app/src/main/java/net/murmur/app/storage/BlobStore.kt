package net.murmur.app.storage

import android.content.Context
import java.io.File

/**
 * Content-addressed blob storage backed by the app's private files directory.
 *
 * Each blob is stored at: `<filesDir>/blobs/<aa>/<bbcc…>` where `<aa>` is the
 * first two hex chars of the blake3 hash and the remainder forms the filename.
 * This mirrors the layout used by `murmurd` on the desktop.
 */
class BlobStore(context: Context) {

    private val blobRoot: File = File(context.filesDir, "blobs")

    init {
        blobRoot.mkdirs()
    }

    /** Store [data] under its blake3 [hash] (raw 32 bytes). */
    fun store(hash: ByteArray, data: ByteArray) {
        val (dir, file) = pathFor(hash)
        dir.mkdirs()
        file.writeBytes(data)
    }

    /**
     * Load the blob for [hash].
     * Returns `null` if the blob is not stored locally.
     */
    fun load(hash: ByteArray): ByteArray? {
        val (_, file) = pathFor(hash)
        return if (file.exists()) file.readBytes() else null
    }

    /** Returns `true` if the blob is present locally. */
    fun contains(hash: ByteArray): Boolean {
        val (_, file) = pathFor(hash)
        return file.exists()
    }

    /** Delete all locally stored blobs.  Used in tests. */
    fun clear() {
        blobRoot.deleteRecursively()
        blobRoot.mkdirs()
    }

    // -----------------------------------------------------------------------

    private fun pathFor(hash: ByteArray): Pair<File, File> {
        val hex = hash.joinToString("") { "%02x".format(it) }
        val prefix = hex.take(2)
        val rest = hex.drop(2)
        val dir = File(blobRoot, prefix)
        return Pair(dir, File(dir, rest))
    }
}
