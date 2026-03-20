package net.murmur.app.db

import androidx.room.Entity
import androidx.room.PrimaryKey

/**
 * Persisted DAG entry.
 *
 * [hash] is the hex-encoded blake3 hash of the entry (64 chars, primary key).
 * [data] is the raw postcard-encoded bytes returned by `MurmurHandle.loadDagEntry`.
 */
@Entity(tableName = "dag_entries")
data class DagEntryEntity(
    @PrimaryKey val hash: String,
    val data: ByteArray
) {
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is DagEntryEntity) return false
        return hash == other.hash && data.contentEquals(other.data)
    }

    override fun hashCode(): Int = 31 * hash.hashCode() + data.contentHashCode()
}
