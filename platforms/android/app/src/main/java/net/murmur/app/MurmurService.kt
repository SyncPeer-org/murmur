package net.murmur.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import net.murmur.app.ui.MainActivity

private const val TAG = "MurmurService"
private const val CHANNEL_ID = "murmur_sync"
private const val NOTIF_ID = 1

/**
 * Android Foreground Service that owns the [MurmurEngine] for the lifetime of
 * the sync session.
 *
 * Lifecycle:
 *  1. [BootReceiver] or [MainActivity] calls `startForegroundService(intent)`.
 *  2. [onStartCommand] creates/restores the engine from shared prefs and Room,
 *     shows the persistent notification, then calls [MurmurEngine.start].
 *  3. UI components bind via [LocalBinder] to access the engine.
 *  4. On SIGTERM / explicit stop, [onDestroy] calls [MurmurEngine.stop].
 */
class MurmurService : Service() {

    // Engine is nullable: it's `null` until the device has been initialized.
    private var engine: MurmurEngine? = null
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // -----------------------------------------------------------------
    // Binder
    // -----------------------------------------------------------------

    inner class LocalBinder : Binder() {
        fun getEngine(): MurmurEngine? = engine
        fun getService(): MurmurService = this@MurmurService
    }

    fun getEngine(): MurmurEngine? = engine

    private val binder = LocalBinder()

    override fun onBind(intent: Intent): IBinder = binder

    // -----------------------------------------------------------------
    // Service lifecycle
    // -----------------------------------------------------------------

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        startForeground(NOTIF_ID, buildNotification("Starting…"))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.i(TAG, "onStartCommand: starting engine")
        val prefs = getSharedPreferences("murmur", MODE_PRIVATE)
        val mnemonic = prefs.getString("mnemonic", null)
        val deviceName = prefs.getString("device_name", "Android Device")!!
        val isCreator = prefs.getBoolean("is_creator", false)

        if (mnemonic == null) {
            Log.w(TAG, "No mnemonic found — waiting for initialization")
            updateNotification("Waiting for setup…")
            return START_STICKY
        }

        serviceScope.launch {
            try {
                val eng = if (isCreator) {
                    MurmurEngine.createNetwork(applicationContext, deviceName, mnemonic)
                } else {
                    MurmurEngine.joinNetwork(applicationContext, deviceName, mnemonic)
                }
                eng.loadPersistedEntries()
                eng.start()
                engine = eng
                updateNotification("Syncing…")
                Log.i(TAG, "Engine started, device=${eng.deviceIdHex()}")

                // Observe engine events and handle them.
                eng.events.collect { event ->
                    handleEngineEvent(event)
                }
            } catch (e: Exception) {
                Log.e(TAG, "Failed to start engine: ${e.message}")
                updateNotification("Error: ${e.message}")
            }
        }

        return START_STICKY
    }

    override fun onDestroy() {
        Log.i(TAG, "onDestroy: stopping engine")
        engine?.stop()
        serviceScope.cancel()
        super.onDestroy()
    }

    // -----------------------------------------------------------------
    // Public API (called from bound UI)
    // -----------------------------------------------------------------

    /**
     * Initialize this device as the creator of a new network.
     * Persists credentials and (re)starts the engine.
     */
    fun initializeNetwork(deviceName: String, mnemonic: String) {
        // Creating a fresh network — wipe any stale DAG data from previous networks.
        clearPersistedData()
        saveCredentials(deviceName, mnemonic, isCreator = true)
        restartEngine()
    }

    /** Initialize this device as a joiner of an existing network. */
    fun joinExistingNetwork(deviceName: String, mnemonic: String) {
        // Joining a new network — wipe stale data from any previous network.
        clearPersistedData()
        saveCredentials(deviceName, mnemonic, isCreator = false)
        restartEngine()
    }

    /** Return the stored device name, or null if not initialized. */
    fun getDeviceName(): String? =
        getSharedPreferences("murmur", MODE_PRIVATE).getString("device_name", null)

    /** Disconnect: stop engine, wipe all credentials and persisted DAG data. */
    fun disconnect() {
        engine?.stop()
        engine = null
        getSharedPreferences("murmur", MODE_PRIVATE).edit().clear().apply()
        clearPersistedData()
        updateNotification("Waiting for setup…")
    }

    // -----------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------

    private fun handleEngineEvent(event: uniffi.murmur.MurmurEventFfi) {
        Log.d(TAG, "Event: $event")
        // Broadcast to any bound UI observers via the engine's SharedFlow.
        // ViewModels observe eng.events directly; no explicit broadcast needed here.
    }

    /** Delete all DAG entries from Room and all blobs from disk. */
    private fun clearPersistedData() {
        serviceScope.launch {
            try {
                net.murmur.app.db.AppDatabase.getInstance(applicationContext)
                    .dagEntryDao().deleteAll()
                net.murmur.app.storage.BlobStore(applicationContext).clear()
                Log.i(TAG, "Cleared persisted DAG entries and blobs")
            } catch (e: Exception) {
                Log.e(TAG, "Failed to clear persisted data: ${e.message}")
            }
        }
    }

    private fun saveCredentials(deviceName: String, mnemonic: String, isCreator: Boolean) {
        getSharedPreferences("murmur", MODE_PRIVATE).edit()
            .putString("mnemonic", mnemonic)
            .putString("device_name", deviceName)
            .putBoolean("is_creator", isCreator)
            .apply()
    }

    private fun restartEngine() {
        engine?.stop()
        engine = null
        val intent = Intent(this, MurmurService::class.java)
        stopSelf()
        startForegroundService(intent)
    }

    // -----------------------------------------------------------------
    // Notification helpers
    // -----------------------------------------------------------------

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Murmur Sync",
            NotificationManager.IMPORTANCE_LOW
        ).apply { description = "Background sync status" }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun buildNotification(text: String): Notification {
        val tapIntent = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Murmur")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentIntent(tapIntent)
            .setOngoing(true)
            .build()
    }

    private fun updateNotification(text: String) {
        val nm = getSystemService(NotificationManager::class.java)
        nm.notify(NOTIF_ID, buildNotification(text))
    }
}
