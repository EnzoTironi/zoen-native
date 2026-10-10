package xyz.tironi.zoen.miniapps

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.CalendarContract
import android.provider.ContactsContract
import android.util.Base64
import android.view.HapticFeedbackConstants
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.exifinterface.media.ExifInterface
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import kotlin.coroutines.resume
import kotlin.math.round
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject

/** Photo tokens are scoped to one open mini-app and can only enter shared state after confirmation. */
internal class MiniAppPhotoVault {
    private val images = linkedMapOf<String, String>()
    fun captureResult(capability: String, result: Any): Any {
        fun bind(image: JSONObject): JSONObject {
            val url = image.getString("dataUrl")
            require(url.startsWith("data:image/jpeg;base64,") && url.length <= 150_000)
            val token = "zoen-photo:${UUID.randomUUID()}"
            if (images.size >= 36) images.remove(images.keys.first())
            images[token] = url
            return JSONObject(image.toString()).put("token", token)
        }
        return when {
            capability == "photos.pick" && result is JSONArray -> JSONArray((0 until result.length()).map { bind(result.getJSONObject(it)) })
            capability == "camera.capture" && result is JSONObject -> bind(result)
            else -> result
        }
    }
    fun resolve(token: String) = images[token]
    fun replace(value: Any): Any = when (value) {
        is String -> if (value.startsWith("zoen-photo:")) requireNotNull(resolve(value)) else value
        is JSONObject -> JSONObject().apply { value.keys().forEach { key -> put(key, replace(value.get(key))) } }
        is JSONArray -> JSONArray((0 until value.length()).map { replace(value.get(it)) })
        else -> value
    }
    fun clear() { images.clear() }
}

class MiniAppDeviceActions internal constructor(private val context: Context) {
    internal var pickPhotos: (CompletableDeferred<List<Uri>>) -> Unit = {}
    internal var takePhoto: (Uri, CompletableDeferred<Boolean>) -> Unit = { _, _ -> }
    internal var pickContact: (CompletableDeferred<Uri?>) -> Unit = {}
    internal var requestPermissions: (Array<String>, CompletableDeferred<Boolean>) -> Unit = { _, _ -> }
    internal var addCalendar: (Intent, CompletableDeferred<Unit>) -> Unit = { _, _ -> }
    val available: Set<String> get() = buildSet {
        addAll(listOf("photos.pick", "contacts.pick", "location", "location.approximate", "calendar.freebusy", "calendar.events", "calendar.add", "clipboard.read", "clipboard.write", "share", "haptics"))
        if (context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)) add("camera.capture")
    }

    suspend fun perform(capability: String, params: JSONObject): Any = when (capability) {
        "photos.pick" -> {
            val done = CompletableDeferred<List<Uri>>()
            pickPhotos(done)
            val images = done.await().take(params.optInt("max", 4).coerceIn(1, 6)).map { image(it, params.optInt("maxSide", 1_024).coerceIn(320, 1_600)) }
            JSONArray(images)
        }
        "camera.capture" -> {
            val dir = File(context.cacheDir, "exports/miniapp-camera").apply { mkdirs() }
            val file = File(dir, "${UUID.randomUUID()}.jpg")
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
            try {
                val done = CompletableDeferred<Boolean>()
                takePhoto(uri, done)
                require(done.await()) { context.getString(xyz.tironi.zoen.R.string.miniapp_picker_cancelled) }
                image(uri, 1_024)
            } finally { file.delete() }
        }
        "contacts.pick" -> {
            val done = CompletableDeferred<Uri?>(); pickContact(done)
            val uri = done.await()
            if (uri == null) JSONArray() else withContext(Dispatchers.IO) {
                require(uri.scheme == "content" && uri.authority == ContactsContract.AUTHORITY)
                val name = context.contentResolver.query(uri, arrayOf(ContactsContract.Contacts.DISPLAY_NAME_PRIMARY), null, null, null)?.use { if (it.moveToFirst()) it.getString(0).take(120) else null }
                JSONArray().apply { if (name != null) put(JSONObject().put("name", name).put("initials", name.split(' ').filter(String::isNotEmpty).take(2).map { it.first() }.joinToString(""))) }
            }
        }
        "location", "location.approximate" -> location(capability == "location.approximate")
        "calendar.freebusy", "calendar.events" -> calendar(params, capability == "calendar.events")
        "calendar.add" -> {
            val title = params.optString("title").take(120)
            require(title.isNotBlank())
            val start = params.optLong("startMs", System.currentTimeMillis())
            val end = params.optLong("endMs", start + 3_600_000L).coerceAtLeast(start + 60_000L)
            require(start in 0..253_402_300_799_999L && end - start <= 31L * 86_400_000L)
            val intent = Intent(Intent.ACTION_INSERT).setData(CalendarContract.Events.CONTENT_URI)
                .putExtra(CalendarContract.Events.TITLE, title).putExtra(CalendarContract.EXTRA_EVENT_BEGIN_TIME, start)
                .putExtra(CalendarContract.EXTRA_EVENT_END_TIME, end).putExtra(CalendarContract.Events.EVENT_LOCATION, params.optString("location").take(160))
                .putExtra(CalendarContract.Events.DESCRIPTION, params.optString("notes").take(1_000))
            val done = CompletableDeferred<Unit>(); addCalendar(intent, done); done.await()
            // System calendar editors do not reliably report whether Save was pressed.
            JSONObject().put("editorOpened", true).put("saved", JSONObject.NULL)
        }
        "clipboard.read" -> {
            val clipboard = context.getSystemService(ClipboardManager::class.java)
            JSONObject().put("text", clipboard.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString()?.take(4_000).orEmpty())
        }
        "clipboard.write" -> {
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Zoen mini-app", params.optString("text").take(4_000)))
            JSONObject().put("copied", true)
        }
        "share" -> {
            val text = params.optString("text").take(4_000)
            require(text.isNotBlank())
            context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null))
            JSONObject().put("opened", true)
        }
        "haptics" -> JSONObject().put("performed", false)
        else -> error("Unavailable native capability")
    }

    private suspend fun permission(permissions: Array<String>) {
        if (permissions.any { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }) return
        val done = CompletableDeferred<Boolean>(); requestPermissions(permissions, done)
        require(done.await()) { context.getString(xyz.tironi.zoen.R.string.miniapp_permission_denied) }
    }

    private suspend fun image(uri: Uri, maxSide: Int): JSONObject = withContext(Dispatchers.IO) {
        require(uri.scheme == "content")
        val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBounded(20 * 1_024 * 1_024) } ?: error("Could not read the photo")
        require(bytes.size <= 20 * 1_024 * 1_024)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        require(bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 160_000_000L)
        val options = BitmapFactory.Options().apply {
            inSampleSize = generateSequence(1) { it * 2 }.first { bounds.outWidth / it <= maxSide * 2 && bounds.outHeight / it <= maxSide * 2 }
        }
        val original = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: error("Could not decode the photo")
        val orientation = runCatching { ExifInterface(java.io.ByteArrayInputStream(bytes)).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }.getOrDefault(1)
        val matrix = android.graphics.Matrix().apply {
            when (orientation) {
                2 -> setScale(-1f, 1f)
                3 -> setRotate(180f)
                4 -> { setRotate(180f); postScale(-1f, 1f) }
                5 -> { setRotate(90f); postScale(-1f, 1f) }
                6 -> setRotate(90f)
                7 -> { setRotate(-90f); postScale(-1f, 1f) }
                8 -> setRotate(-90f)
            }
        }
        val decoded = if (orientation in 2..8) Bitmap.createBitmap(original, 0, 0, original.width, original.height, matrix, true) else original
        try {
            var side = maxSide
            repeat(6) {
                val scale = (side.toFloat() / maxOf(decoded.width, decoded.height)).coerceAtMost(1f)
                val small = Bitmap.createScaledBitmap(decoded, (decoded.width * scale).toInt().coerceAtLeast(1), (decoded.height * scale).toInt().coerceAtLeast(1), true)
                try {
                    val output = ByteArrayOutputStream(); small.compress(Bitmap.CompressFormat.JPEG, 62, output)
                    val url = "data:image/jpeg;base64," + Base64.encodeToString(output.toByteArray(), Base64.NO_WRAP)
                    if (url.length <= 150_000) return@withContext JSONObject().put("dataUrl", url).put("width", small.width).put("height", small.height).put("locationStripped", true)
                } finally { if (small !== decoded) small.recycle() }
                side = (side * .75).toInt()
            }
            error("Could not resize the photo")
        } finally { decoded.recycle(); if (original !== decoded) original.recycle() }
    }

    private suspend fun location(approximate: Boolean): JSONObject {
        val permissions = if (approximate) arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION) else arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)
        permission(permissions)
        val coarse = approximate || ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED
        val manager = context.getSystemService(LocationManager::class.java)
        val provider = if (!coarse && manager.isProviderEnabled(LocationManager.GPS_PROVIDER)) LocationManager.GPS_PROVIDER else LocationManager.NETWORK_PROVIDER
        require(manager.isProviderEnabled(provider)) { context.getString(xyz.tironi.zoen.R.string.miniapp_location_off) }
        val fix = try { withTimeout(12_000) {
            suspendCancellableCoroutine<Location> { continuation ->
                val listener = object : LocationListener {
                    override fun onLocationChanged(location: Location) { manager.removeUpdates(this); if (continuation.isActive) continuation.resume(location) }
                    @Deprecated("Platform compatibility") override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) {}
                    override fun onProviderDisabled(provider: String) { manager.removeUpdates(this); continuation.cancel() }
                }
                try { @Suppress("DEPRECATION", "MissingPermission") manager.requestSingleUpdate(provider, listener, android.os.Looper.getMainLooper()) }
                catch (error: Exception) { continuation.cancel(error) }
                continuation.invokeOnCancellation { manager.removeUpdates(listener) }
            }
        }
        } catch (_: kotlinx.coroutines.TimeoutCancellationException) { error("Location took too long") }
        return JSONObject().put("lat", if (coarse) round(fix.latitude / .02) * .02 else fix.latitude)
            .put("lon", if (coarse) round(fix.longitude / .02) * .02 else fix.longitude).put("accuracyM", if (coarse) maxOf(3_000f, fix.accuracy) else fix.accuracy).put("approximate", coarse)
    }

    private suspend fun calendar(params: JSONObject, details: Boolean): JSONArray {
        permission(arrayOf(Manifest.permission.READ_CALENDAR))
        return withContext(Dispatchers.IO) {
            val start = params.optLong("startMs", System.currentTimeMillis())
            val end = params.optLong("endMs", start + 7 * 86_400_000L).coerceAtMost(start + 31 * 86_400_000L)
            require(start >= 0 && end > start)
            val uri = CalendarContract.Instances.CONTENT_URI.buildUpon().appendPath(start.toString()).appendPath(end.toString()).build()
            val projection = mutableListOf(CalendarContract.Instances.BEGIN, CalendarContract.Instances.END, CalendarContract.Instances.ALL_DAY, CalendarContract.Instances.AVAILABILITY)
            if (details) projection += listOf(CalendarContract.Instances.TITLE, CalendarContract.Instances.EVENT_LOCATION, CalendarContract.Instances.DESCRIPTION)
            val blocks = mutableListOf<Pair<Long, Long>>()
            val result = JSONArray()
            context.contentResolver.query(uri, projection.toTypedArray(), null, null, "${CalendarContract.Instances.BEGIN} ASC")?.use { cursor ->
                while (cursor.moveToNext() && cursor.position < 1_000) {
                    if (cursor.getInt(2) != 0 || cursor.getInt(3) == CalendarContract.Events.AVAILABILITY_FREE) continue
                    val begin = cursor.getLong(0).coerceAtLeast(start); val finish = cursor.getLong(1).coerceAtMost(end)
                    if (details) result.put(JSONObject().put("startMs", begin).put("endMs", finish).put("title", cursor.getString(4).orEmpty().take(120)).put("location", cursor.getString(5).orEmpty().take(160)).put("notes", cursor.getString(6).orEmpty().take(1_000)))
                    else if (blocks.lastOrNull()?.second?.let { begin <= it } == true) blocks[blocks.lastIndex] = blocks.last().first to maxOf(blocks.last().second, finish) else blocks += begin to finish
                }
            }
            if (!details) blocks.forEach { result.put(JSONObject().put("startMs", it.first).put("endMs", it.second)) }
            result
        }
    }
}

@Composable
fun rememberMiniAppDeviceActions(): MiniAppDeviceActions {
    val context = LocalContext.current
    val actions = remember(context) { MiniAppDeviceActions(context) }
    var photos by remember { mutableStateOf<CompletableDeferred<List<Uri>>?>(null) }
    var camera by remember { mutableStateOf<CompletableDeferred<Boolean>?>(null) }
    var contacts by remember { mutableStateOf<CompletableDeferred<Uri?>?>(null) }
    var permissions by remember { mutableStateOf<CompletableDeferred<Boolean>?>(null) }
    var calendar by remember { mutableStateOf<CompletableDeferred<Unit>?>(null) }
    val photoLauncher = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(6)) { photos?.complete(it); photos = null }
    val cameraLauncher = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { camera?.complete(it); camera = null }
    val contactLauncher = rememberLauncherForActivityResult(ActivityResultContracts.PickContact()) { contacts?.complete(it); contacts = null }
    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { permissions?.complete(it.values.any { granted -> granted }); permissions = null }
    val calendarLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { calendar?.complete(Unit); calendar = null }
    SideEffect {
        actions.pickPhotos = { done -> photos = done; photoLauncher.launch(androidx.activity.result.PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
        actions.takePhoto = { uri, done -> camera = done; cameraLauncher.launch(uri) }
        actions.pickContact = { done -> contacts = done; contactLauncher.launch(null) }
        actions.requestPermissions = { requested, done -> permissions = done; permissionLauncher.launch(requested) }
        actions.addCalendar = { intent, done -> calendar = done; calendarLauncher.launch(intent) }
    }
    DisposableEffect(actions) { onDispose { photos?.cancel(); camera?.cancel(); contacts?.cancel(); permissions?.cancel(); calendar?.cancel() } }
    return actions
}
