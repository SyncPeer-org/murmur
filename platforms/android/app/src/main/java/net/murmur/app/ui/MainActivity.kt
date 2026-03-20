package net.murmur.app.ui

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Bundle
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Devices
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import net.murmur.app.DeviceViewModel
import net.murmur.app.FileViewModel
import net.murmur.app.MurmurEngine
import net.murmur.app.MurmurService

/**
 * Single-activity host.  Uses Jetpack Compose for all UI.
 *
 * Tabs:
 *  - **Devices** — approve/revoke, list all devices
 *  - **Files**   — browse and upload synced files
 *  - **Status**  — device ID, DAG info, event log
 */
class MainActivity : ComponentActivity() {

    private var murmurService: MurmurService? = null
    private var serviceEngine: MurmurEngine? = null

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            val localBinder = binder as MurmurService.LocalBinder
            murmurService = localBinder.getService()
            serviceEngine = localBinder.getEngine()
        }

        override fun onServiceDisconnected(name: ComponentName) {
            murmurService = null
            serviceEngine = null
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Start + bind to the service.
        val serviceIntent = Intent(this, MurmurService::class.java)
        startForegroundService(serviceIntent)
        bindService(serviceIntent, serviceConnection, Context.BIND_AUTO_CREATE)

        setContent {
            MurmurTheme {
                var selectedTab by remember { mutableIntStateOf(0) }

                // Obtain engine; may be null until service connects.
                val engine = serviceEngine
                val prefs = getSharedPreferences("murmur", Context.MODE_PRIVATE)
                val initialized = prefs.contains("mnemonic")

                Scaffold(
                    bottomBar = {
                        if (engine != null) {
                            NavigationBar {
                                NavigationBarItem(
                                    selected = selectedTab == 0,
                                    onClick = { selectedTab = 0 },
                                    icon = { Icon(Icons.Default.Devices, "Devices") },
                                    label = { Text("Devices") }
                                )
                                NavigationBarItem(
                                    selected = selectedTab == 1,
                                    onClick = { selectedTab = 1 },
                                    icon = { Icon(Icons.Default.Folder, "Files") },
                                    label = { Text("Files") }
                                )
                                NavigationBarItem(
                                    selected = selectedTab == 2,
                                    onClick = { selectedTab = 2 },
                                    icon = { Icon(Icons.Default.Info, "Status") },
                                    label = { Text("Status") }
                                )
                            }
                        }
                    }
                ) { innerPadding ->
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(innerPadding)
                    ) {
                        when {
                            engine == null || !initialized -> SetupScreen(
                                onCreateNetwork = { name, mnemonic ->
                                    murmurService?.initializeNetwork(name, mnemonic)
                                },
                                onJoinNetwork = { name, mnemonic ->
                                    murmurService?.joinExistingNetwork(name, mnemonic)
                                }
                            )

                            selectedTab == 0 -> DeviceScreen(
                                viewModel = remember { DeviceViewModel(engine) }
                            )

                            selectedTab == 1 -> FileScreen(
                                viewModel = remember { FileViewModel(engine) }
                            )

                            else -> StatusScreen(
                                deviceIdHex = engine.deviceIdHex(),
                                events = engine.events
                            )
                        }
                    }
                }
            }
        }
    }

    override fun onDestroy() {
        unbindService(serviceConnection)
        super.onDestroy()
    }
}
