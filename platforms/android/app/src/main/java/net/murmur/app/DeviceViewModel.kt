package net.murmur.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import net.murmur.generated.DeviceInfoFfi
import net.murmur.generated.MurmurEventFfi

/**
 * ViewModel that manages the device list and pending join requests.
 *
 * Observes [MurmurEngine.events] and refreshes device lists on relevant events.
 */
class DeviceViewModel(private val engine: MurmurEngine) : ViewModel() {

    private val _devices = MutableStateFlow<List<DeviceInfoFfi>>(emptyList())
    /** All known devices (approved + pending). */
    val devices: StateFlow<List<DeviceInfoFfi>> = _devices.asStateFlow()

    private val _pendingRequests = MutableStateFlow<List<DeviceInfoFfi>>(emptyList())
    /** Devices waiting for approval. */
    val pendingRequests: StateFlow<List<DeviceInfoFfi>> = _pendingRequests.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        refresh()
        observeEvents()
    }

    /** Approve a pending device. */
    fun approveDevice(deviceIdHex: String, role: String = "full") {
        viewModelScope.launch {
            try {
                engine.approveDevice(deviceIdHex, role)
                refresh()
            } catch (e: Exception) {
                _error.value = "Failed to approve device: ${e.message}"
            }
        }
    }

    /** Revoke an approved device. */
    fun revokeDevice(deviceIdHex: String) {
        viewModelScope.launch {
            try {
                engine.revokeDevice(deviceIdHex)
                refresh()
            } catch (e: Exception) {
                _error.value = "Failed to revoke device: ${e.message}"
            }
        }
    }

    fun clearError() { _error.value = null }

    // -----------------------------------------------------------------------

    private fun refresh() {
        viewModelScope.launch {
            _devices.value = engine.listDevices()
            _pendingRequests.value = engine.pendingRequests()
        }
    }

    private fun observeEvents() {
        viewModelScope.launch {
            engine.events.collect { event ->
                when (event) {
                    is MurmurEventFfi.DeviceJoinRequested,
                    is MurmurEventFfi.DeviceApproved,
                    is MurmurEventFfi.DeviceRevoked -> refresh()
                    else -> Unit
                }
            }
        }
    }
}
