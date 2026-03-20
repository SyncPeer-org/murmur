package net.murmur.app

import android.app.Application
import android.util.Log

/**
 * Application subclass.  Performs one-time app-level initialization.
 */
class MurmurApp : Application() {

    override fun onCreate() {
        super.onCreate()
        Log.i("MurmurApp", "Application started")
    }
}
