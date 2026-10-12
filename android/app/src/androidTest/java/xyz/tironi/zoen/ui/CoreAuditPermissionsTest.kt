package xyz.tironi.zoen.ui

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.core.*

@RunWith(AndroidJUnit4::class)
class CoreAuditPermissionsTest {
    @Test fun fullSignedHistoryAndPermissionRevocationsSurviveRestart() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val directory = File(context.noBackupFilesDir, "audit-${UUID.randomUUID()}").apply { mkdirs() }
        val path = File(directory, "audit.sqlite").absolutePath
        var core = RodaEngine.open(path, "en")
        try {
            core.seedDemoIfEmpty()
            val space = core.spaces().first { it.members.any { member -> member.isMe } }
            repeat(35) { core.sendMessage(space.id, "Audit event $it") }
            val events = core.logEvents(space.id)
            assertTrue(events.size > 35)
            val sent = events.filter { it.author.isMe }.take(35)
            assertEquals(35, sent.size)
            assertTrue(sent.all { it.atMs > 0 && it.hash.isNotBlank() && it.prev.isNotBlank() && it.signature.isNotBlank() })
            events.zipWithNext().forEach { (newer, older) ->
                assertEquals(older.hash, newer.prev)
                assertEquals(older.seq + 1uL, newer.seq)
            }
            val report = core.verifyLog(space.id)
            assertTrue(report.error, report.valid)
            assertEquals(events.size.toULong(), report.events)
            assertEquals(events.first().hash, report.headHash)

            val request = core.requests().first { it.status == RequestStatus.PENDING && it.agent.isMine }
            val decision = core.decideRequest(request.id, RequestDecision.ALWAYS_DENY)
            val standingId = checkNotNull(decision.standingGrantId)
            assertEquals(listOf(standingId), standingInScope(core.standingDecisions(), request.agent.id, request.spaceId).filter { it.grantId == standingId }.map { it.grantId })
            val standingBefore = core.logEvents(request.spaceId).size
            core.revokeStanding(standingId)
            assertFalse(core.standingDecisions().any { it.grantId == standingId })
            assertEquals(standingBefore + 1, core.logEvents(request.spaceId).size)

            val app = core.items().first { it.app != null }
            val deviceId = core.grantAppDevice(app.id, "location.approximate", "Find a nearby meeting point", true)
            val onceId = core.grantAppDevice(app.id, "photos.pick", "Share the photo you choose", false)
            val scopes = mapOf(app.id to PermissionItemScope(app.spaceId, app.createdBy.id))
            val device = deviceInScope(core.appDeviceGrants(), scopes, null, app.spaceId)
            assertTrue(device.any { it.grantId == deviceId && it.always })
            assertTrue(device.any { it.grantId == onceId && !it.always })
            assertTrue(deviceInScope(device, scopes, null, "a-different-chat").isEmpty())
            val deviceBefore = core.logEvents(app.spaceId).size
            core.revokeAppDevice(deviceId)
            core.revokeAppDevice(onceId)
            assertFalse(core.appDeviceGrants().any { it.grantId == deviceId || it.grantId == onceId })
            assertEquals(deviceBefore + 2, core.logEvents(app.spaceId).size)
            assertTrue(core.verifyAll().all { it.valid })

            val before = core.logEvents(space.id).size
            core.destroy()
            core = RodaEngine.open(path, "en")
            assertEquals(before, core.logEvents(space.id).size)
            assertFalse(core.standingDecisions().any { it.grantId == standingId })
            assertFalse(core.appDeviceGrants().any { it.grantId == deviceId || it.grantId == onceId })
            assertTrue(core.verifyAll().all { it.valid })
        } finally { core.destroy(); directory.deleteRecursively() }
    }
}
