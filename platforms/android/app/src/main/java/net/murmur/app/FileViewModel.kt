package net.murmur.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import net.murmur.generated.MurmurEventFfi

/**
 * ViewModel that tracks synced file metadata.
 *
 * Files are represented as a list of [SyncedFile] items derived from engine
 * events.  In a full implementation this would query the engine's
 * `listFiles()` method; for now we accumulate [MurmurEventFfi.FileSynced]
 * events.
 */
class FileViewModel(private val engine: MurmurEngine) : ViewModel() {

    data class SyncedFile(val blobHash: ByteArray, val filename: String) {
        override fun equals(other: Any?): Boolean {
            if (this === other) return true
            if (other !is SyncedFile) return false
            return blobHash.contentEquals(other.blobHash)
        }
        override fun hashCode(): Int = blobHash.contentHashCode()
    }

    private val _files = MutableStateFlow<List<SyncedFile>>(emptyList())
    /** All synced files (newest first). */
    val files: StateFlow<List<SyncedFile>> = _files.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        observeEvents()
    }

    /** Upload a file from the local filesystem. */
    fun addFile(blobHash: ByteArray, metadata: net.murmur.generated.FileMetadataFfi, data: ByteArray) {
        viewModelScope.launch {
            try {
                engine.addFile(blobHash, metadata, data)
            } catch (e: Exception) {
                _error.value = "Failed to add file: ${e.message}"
            }
        }
    }

    fun clearError() { _error.value = null }

    // -----------------------------------------------------------------------

    private fun observeEvents() {
        viewModelScope.launch {
            engine.events.collect { event ->
                if (event is MurmurEventFfi.FileSynced) {
                    val hash = event.blobHash.toByteArray()
                    val filename = event.filename
                    val newFile = SyncedFile(hash, filename)
                    _files.value = (listOf(newFile) + _files.value).distinctBy {
                        it.blobHash.toList()
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------

private fun List<UByte>.toByteArray(): ByteArray = ByteArray(size) { this[it].toByte() }
