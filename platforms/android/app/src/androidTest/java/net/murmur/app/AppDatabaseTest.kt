package net.murmur.app

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import net.murmur.app.db.AppDatabase
import net.murmur.app.db.DagEntryDao
import net.murmur.app.db.DagEntryEntity
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Instrumented tests for [AppDatabase] + [DagEntryDao].
 *
 * Tests:
 * - [testInsertAndLoad] — insert then loadAll roundtrip
 * - [testMultipleEntriesPersist] — multiple entries survive reopen
 * - [testReplaceOnConflict] — inserting same hash replaces data
 */
@RunWith(AndroidJUnit4::class)
class AppDatabaseTest {

    private lateinit var db: AppDatabase
    private lateinit var dao: DagEntryDao

    @Before
    fun setup() {
        val ctx: Context = ApplicationProvider.getApplicationContext()
        db = Room.inMemoryDatabaseBuilder(ctx, AppDatabase::class.java).build()
        dao = db.dagEntryDao()
    }

    @After
    fun teardown() {
        db.close()
    }

    @Test
    fun testInsertAndLoad() = runBlocking {
        val entry = DagEntryEntity(hash = "aabbccdd", data = byteArrayOf(1, 2, 3))
        dao.insert(entry)
        val all = dao.loadAll()
        assertEquals(1, all.size)
        assertEquals("aabbccdd", all[0].hash)
    }

    @Test
    fun testMultipleEntriesPersist() = runBlocking {
        repeat(5) { i ->
            dao.insert(DagEntryEntity(hash = "hash$i", data = byteArrayOf(i.toByte())))
        }
        assertEquals(5, dao.count())
    }

    @Test
    fun testReplaceOnConflict() = runBlocking {
        dao.insert(DagEntryEntity(hash = "abc", data = byteArrayOf(1)))
        dao.insert(DagEntryEntity(hash = "abc", data = byteArrayOf(2))) // replaces
        val all = dao.loadAll()
        assertEquals(1, all.size)
        assertEquals(2, all[0].data[0])
    }
}
