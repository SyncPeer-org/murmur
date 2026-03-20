package net.murmur.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Restarts [MurmurService] after a device reboot or app update.
 *
 * Registered for `BOOT_COMPLETED` and `MY_PACKAGE_REPLACED` in the manifest.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED ||
            intent.action == Intent.ACTION_MY_PACKAGE_REPLACED
        ) {
            val serviceIntent = Intent(context, MurmurService::class.java)
            context.startForegroundService(serviceIntent)
        }
    }
}
