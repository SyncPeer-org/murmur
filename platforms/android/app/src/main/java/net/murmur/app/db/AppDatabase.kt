package net.murmur.app.db

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase

/**
 * Room database that persists Murmur DAG entries.
 *
 * The schema is intentionally minimal: each entry is stored as opaque
 * postcard-encoded bytes so the database never needs to know about the
 * internal DAG structure.
 */
@Database(entities = [DagEntryEntity::class], version = 1, exportSchema = false)
abstract class AppDatabase : RoomDatabase() {

    abstract fun dagEntryDao(): DagEntryDao

    companion object {
        @Volatile
        private var INSTANCE: AppDatabase? = null

        fun getInstance(context: Context): AppDatabase =
            INSTANCE ?: synchronized(this) {
                INSTANCE ?: Room.databaseBuilder(
                    context.applicationContext,
                    AppDatabase::class.java,
                    "murmur.db"
                ).build().also { INSTANCE = it }
            }
    }
}
