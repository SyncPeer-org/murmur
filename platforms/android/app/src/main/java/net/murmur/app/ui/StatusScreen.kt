package net.murmur.app.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.SharedFlow
import uniffi.murmur.MurmurEventFfi

/** Status tab: device ID and a scrollable event log. */
@Composable
fun StatusScreen(
    deviceIdHex: String,
    events: SharedFlow<MurmurEventFfi>
) {
    val eventLog = remember { mutableStateListOf<String>() }
    val currentEvents by events.collectAsState(initial = null)
    currentEvents?.let { eventLog.add(0, it.toString()) }

    Column(modifier = Modifier.padding(16.dp)) {
        Text("Status", style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(8.dp))
        Text("Device ID:", style = MaterialTheme.typography.labelMedium)
        Text(
            deviceIdHex,
            style = MaterialTheme.typography.bodySmall,
            fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace
        )
        Spacer(Modifier.height(16.dp))
        Text("Event log (newest first):", style = MaterialTheme.typography.labelMedium)
        Spacer(Modifier.height(4.dp))
        if (eventLog.isEmpty()) {
            Text(
                "No events yet.",
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        } else {
            LazyColumn {
                items(eventLog) { line ->
                    Text(
                        line,
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace
                    )
                }
            }
        }
    }
}
