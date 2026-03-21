package net.murmur.app.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query

/** Data-access object for [DagEntryEntity]. */
@Dao
interface DagEntryDao {

    /** Insert or replace a DAG entry (on conflict with same hash, replace). */
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(entry: DagEntryEntity)

    /** Load all persisted entries.  Called once at startup. */
    @Query("SELECT * FROM dag_entries")
    suspend fun loadAll(): List<DagEntryEntity>

    /** Delete all entries — called when disconnecting from a network. */
    @Query("DELETE FROM dag_entries")
    suspend fun deleteAll()

    /** Number of entries stored — useful for tests. */
    @Query("SELECT COUNT(*) FROM dag_entries")
    suspend fun count(): Int
}
