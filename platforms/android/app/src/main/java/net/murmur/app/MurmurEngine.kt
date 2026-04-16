package net.murmur.app

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch
import net.murmur.app.db.AppDatabase
import net.murmur.app.db.DagEntryEntity
import net.murmur.app.storage.BlobStore
import uniffi.murmur.DeviceInfoFfi
import uniffi.murmur.FileMetadataFfi
import uniffi.murmur.FfiPlatformCallbacks
import uniffi.murmur.MurmurEventFfi
import uniffi.murmur.MurmurHandle
import uniffi.murmur.createNetwork
import uniffi.murmur.joinNetwork

private const val TAG = "MurmurEngine"

/**
 * Kotlin wrapper around [MurmurHandle] (the UniFFI-generated Rust FFI object).
 *
 * Responsibilities:
 *  - Implement [FfiPlatformCallbacks] to bridge Rust callbacks → Android storage
 *  - Expose a [SharedFlow] of [MurmurEventFfi] for UI/ViewModel consumption
 *  - Manage startup (load all DAG entries from Room → call [MurmurHandle.loadDagEntry])
 *  - Expose the handle's API as coroutine-friendly suspend functions
 */
class MurmurEngine private constructor(
    private val handle: MurmurHandle,
    private val db: AppDatabase,
    private val blobStore: BlobStore,
    private val eventFlow: MutableSharedFlow<MurmurEventFfi>
) {
    /** Stream of engine events for UI to observe. */
    val events: SharedFlow<MurmurEventFfi> = eventFlow.asSharedFlow()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /** Start background networking. */
    fun start() = handle.start()

    /** Stop background networking and release resources. */
    fun stop() = handle.stop()

    /** This device's ID as a 64-char lowercase hex string. */
    fun deviceIdHex(): String = handle.deviceIdHex()

    // -----------------------------------------------------------------------
    // Device management
    // -----------------------------------------------------------------------

    /** List all known devices (approved + pending). */
    suspend fun listDevices(): List<DeviceInfoFfi> =
        kotlinx.coroutines.withContext(Dispatchers.IO) { handle.listDevices() }

    /** List devices waiting for approval. */
    suspend fun pendingRequests(): List<DeviceInfoFfi> =
        kotlinx.coroutines.withContext(Dispatchers.IO) { handle.pendingRequests() }

    /**
     * Approve a pending device.
     * @param deviceIdHex 64-char lowercase hex device ID
     */
    suspend fun approveDevice(deviceIdHex: String) =
        kotlinx.coroutines.withContext(Dispatchers.IO) {
            handle.approveDevice(deviceIdHex)
        }

    /** Revoke a device. */
    suspend fun revokeDevice(deviceIdHex: String) =
        kotlinx.coroutines.withContext(Dispatchers.IO) {
            handle.revokeDevice(deviceIdHex)
        }

    // -----------------------------------------------------------------------
    // File management
    // -----------------------------------------------------------------------

    /**
     * Add a file to the network.
     * @param blobHash Raw 32-byte blake3 hash of [data].
     * @param metadata File metadata.
     * @param data Raw file bytes.
     */
    suspend fun addFile(blobHash: ByteArray, metadata: FileMetadataFfi, data: ByteArray) =
        kotlinx.coroutines.withContext(Dispatchers.IO) {
            handle.addFile(blobHash, metadata, data)
        }

    // -----------------------------------------------------------------------
    // Access control
    // -----------------------------------------------------------------------

    /** Request access to files on another device. */
    suspend fun requestAccess(deviceIdHex: String, scope: uniffi.murmur.AccessScopeFfi) =
        kotlinx.coroutines.withContext(Dispatchers.IO) {
            handle.requestAccess(deviceIdHex, scope)
        }

    // -----------------------------------------------------------------------
    // Blob access
    // -----------------------------------------------------------------------

    /**
     * Fetch a blob from local storage.
     * Returns `null` if not available locally.
     */
    fun fetchBlob(blobHash: ByteArray): ByteArray? =
        handle.fetchBlob(blobHash)

    // -----------------------------------------------------------------------
    // Factory
    // -----------------------------------------------------------------------

    companion object {
        /**
         * Create a new Murmur network and return an initialized [MurmurEngine].
         *
         * The [handle] is a new [MurmurHandle]; no entries are loaded from DB
         * (the first device starts fresh).
         */
        fun createNetwork(context: Context, deviceName: String, mnemonic: String): MurmurEngine {
            val db = AppDatabase.getInstance(context)
            val blobStore = BlobStore(context)
            val eventFlow = MutableSharedFlow<MurmurEventFfi>(extraBufferCapacity = 64)
            val callbacks = AndroidCallbacks(db, blobStore, eventFlow)
            val handle = createNetwork(deviceName, mnemonic, callbacks)
            return MurmurEngine(handle, db, blobStore, eventFlow)
        }

        /**
         * Join an existing Murmur network.
         *
         * After construction, call [loadPersistedEntries] to replay any DAG
         * entries from the previous session.
         */
        fun joinNetwork(context: Context, deviceName: String, mnemonic: String): MurmurEngine {
            val db = AppDatabase.getInstance(context)
            val blobStore = BlobStore(context)
            val eventFlow = MutableSharedFlow<MurmurEventFfi>(extraBufferCapacity = 64)
            val callbacks = AndroidCallbacks(db, blobStore, eventFlow)
            val handle = joinNetwork(deviceName, mnemonic, callbacks)
            return MurmurEngine(handle, db, blobStore, eventFlow)
        }
    }

    /**
     * Load all persisted DAG entries from Room into the engine.
     *
     * Must be called once on startup, before [start].
     */
    suspend fun loadPersistedEntries() {
        val entries = db.dagEntryDao().loadAll()
        Log.i(TAG, "Loading ${entries.size} persisted DAG entries")
        kotlinx.coroutines.withContext(Dispatchers.IO) {
            for (entity in entries) {
                try {
                    handle.loadDagEntry(entity.data)
                } catch (e: Exception) {
                    Log.w(TAG, "Failed to load DAG entry ${entity.hash}: ${e.message}")
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Inner platform callbacks implementation
    // -----------------------------------------------------------------------

    private class AndroidCallbacks(
        private val db: AppDatabase,
        private val blobStore: BlobStore,
        private val eventFlow: MutableSharedFlow<MurmurEventFfi>
    ) : FfiPlatformCallbacks {

        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

        override fun onDagEntry(entryBytes: ByteArray) {
            // Compute a simple hex key from the first 8 bytes of the entry bytes.
            val hashKey = entryBytes.take(8).joinToString("") { "%02x".format(it) }
            scope.launch {
                try {
                    db.dagEntryDao().insert(DagEntryEntity(hash = hashKey, data = entryBytes))
                } catch (e: Exception) {
                    Log.e(TAG, "onDagEntry: failed to persist entry: ${e.message}")
                }
            }
        }

        override fun onBlobReceived(blobHash: ByteArray, data: ByteArray) {
            blobStore.store(blobHash, data)
        }

        override fun onBlobNeeded(blobHash: ByteArray): ByteArray? =
            blobStore.load(blobHash)

        override fun onEvent(event: MurmurEventFfi) {
            scope.launch { eventFlow.emit(event) }
        }
    }
}

