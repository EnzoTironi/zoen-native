package xyz.tironi.zoen.agent

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import kotlinx.coroutines.runBlocking
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AndroidSecretVault

@RunWith(AndroidJUnit4::class)
class AgentCryptoTest {
    @Test fun onlyTheRestoredOwnerKeyOpensFramesAndInputsRejectTamperingOrReplay() {
        val key = LiveViewKey.generate()
        val restored = LiveViewKey.restore(key.secret())
        val vm = LiveViewDemoVm.start(key.publicKey())
        val live = restored.accept(vm.vmPub(), vm.session())
        val wrongKey = LiveViewKey.generate()
        val wrong = wrongKey.accept(vm.vmPub(), vm.session())
        try {
            val frame = "private pixels".toByteArray()
            val sealed = vm.sealFrame(frame)
            assertThrows(CoreException::class.java) { wrong.openFrame(sealed) }
            val tampered = sealed.copyOf().apply { this[lastIndex] = (this[lastIndex].toInt() xor 1).toByte() }
            assertThrows(CoreException::class.java) { live.openFrame(tampered) }
            assertArrayEquals(frame, live.openFrame(sealed))
            assertThrows(CoreException::class.java) { live.openFrame(sealed) }
            val input = live.sealText("private-demo-password")
            assertFalse(input.toString(Charsets.UTF_8).contains("private-demo-password"))
            val altered = input.copyOf().apply { this[lastIndex] = (this[lastIndex].toInt() xor 1).toByte() }
            assertThrows(CoreException::class.java) { vm.openInput(altered) }
            assertEquals(21u, (vm.openInput(input) as LiveViewInput.Text).chars)
            assertThrows(CoreException::class.java) { vm.openInput(input) }
            assertEquals(LiveViewInput.Key("Backspace"), vm.openInput(live.sealKey("Backspace")))
            assertEquals(LiveViewInput.Click(120.0, 80.0), vm.openInput(live.sealClick(120.0, 80.0)))
            assertEquals(LiveViewInput.Done, vm.openInput(live.sealDone()))
        } finally { wrong.destroy(); wrongKey.destroy(); live.destroy(); vm.destroy(); restored.destroy(); key.destroy() }
    }

    @Test fun croppedPhotoAndHandleChangesSurviveCoreRestartWithEncryptedMedia() = runBlocking {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "profile-${UUID.randomUUID()}"
        val directory = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val vault = AndroidSecretVault(context, "$namespace-vault")
        val path = File(directory, "profile.sqlite").absolutePath
        var core = RodaEngine.open(path, "en")
        val bitmap = Bitmap.createBitmap(1600, 800, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.BLUE) }
        try {
            val account = core.createAccount("Photo Test", "photo_test", "http://10.0.2.2:18787", vault)
            val photo = ProfilePhotos.jpeg(bitmap, 2f, .5f, -.5f)
            core.updateProfile("New Name", "photo_changed", "A private bio")
            core.updateMyProfile("New Name", "A private bio", PhotoChange.Set(photo, "image/jpeg"))
            val profile = core.getProfile(account.identityId)
            assertTrue(profile.photoReady)
            val hash = checkNotNull(profile.photoSha256)
            assertArrayEquals(photo, core.media(hash))
            assertEquals("photo_changed", core.account()!!.handle)
            core.destroy()
            core = RodaEngine.open(path, "en")
            assertTrue(core.unlock(vault))
            assertArrayEquals(photo, core.media(hash))
            assertEquals("New Name", core.getProfile(account.identityId).name)
            val peer = "ab".repeat(32)
            core.blockPerson(peer)
            assertTrue(core.blockedPeople().contains(peer))
            core.unblockPerson(peer)
            assertFalse(core.blockedPeople().contains(peer))
            core.updateMyProfile("New Name", "A private bio", PhotoChange.Remove)
            assertNull(core.getProfile(account.identityId).photoSha256)
            core.eraseDevice(vault)
        } finally {
            bitmap.recycle(); core.destroy(); directory.deleteRecursively()
            File(context.noBackupFilesDir, "$namespace-vault").deleteRecursively()
        }
    }
}
