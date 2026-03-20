package net.murmur.app

import android.content.Context
import android.content.Intent
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.ServiceTestRule
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

/**
 * Instrumented tests for [MurmurService].
 *
 * Tests:
 * - [testServiceStartsAndBinds] — service can be started and bound
 * - [testBinderReturnsService] — binder exposes the service instance
 */
@RunWith(AndroidJUnit4::class)
class MurmurServiceTest {

    @get:Rule
    val serviceRule = ServiceTestRule()

    private val context: Context = ApplicationProvider.getApplicationContext()

    @Test
    fun testServiceStartsAndBinds() {
        val intent = Intent(context, MurmurService::class.java)
        val binder = serviceRule.bindService(intent)
        assertNotNull(binder)
    }

    @Test
    fun testBinderReturnsService() {
        val intent = Intent(context, MurmurService::class.java)
        val binder = serviceRule.bindService(intent) as MurmurService.LocalBinder
        assertNotNull(binder.getService())
    }
}
