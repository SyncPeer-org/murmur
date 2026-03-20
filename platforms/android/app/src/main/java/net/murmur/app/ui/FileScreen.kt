package net.murmur.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import net.murmur.app.FileViewModel

/** Screen showing all synced files. */
@Composable
fun FileScreen(viewModel: FileViewModel) {
    val files by viewModel.files.collectAsState()
    val error by viewModel.error.collectAsState()

    Column(modifier = Modifier.padding(16.dp)) {
        Text("Files (${files.size})", style = MaterialTheme.typography.titleMedium)
        error?.let {
            Spacer(Modifier.height(8.dp))
            Text(it, color = MaterialTheme.colorScheme.error)
        }
        Spacer(Modifier.height(8.dp))
        if (files.isEmpty()) {
            Text(
                "No files synced yet. Photos taken on your device will appear here.",
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        } else {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(files) { file ->
                    FileCard(file)
                }
            }
        }
    }
}

@Composable
private fun FileCard(file: FileViewModel.SyncedFile) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(12.dp)) {
            Text(file.filename, style = MaterialTheme.typography.bodyLarge)
            Text(
                "Hash: ${file.blobHash.take(4).joinToString("") { "%02x".format(it) }}…",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}
