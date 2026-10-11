package xyz.tironi.zoen.agent

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.graphics.Typeface
import java.io.ByteArrayOutputStream
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.core.LiveViewDemoVm
import xyz.tironi.zoen.core.LiveViewInput
import xyz.tironi.zoen.core.LiveViewKey
import xyz.tironi.zoen.core.LiveViewSession
import xyz.tironi.zoen.core.Persona
import xyz.tironi.zoen.data.AndroidSecretVault

enum class BrowserPhase { Browsing, NeedsYou, Waiting, Driving, Finished }

data class BrowserOwnership(val phase: BrowserPhase = BrowserPhase.Browsing, val typed: Int = 0) {
    fun needsYou() = if (phase == BrowserPhase.Browsing) copy(phase = BrowserPhase.NeedsYou) else this
    fun notNow() = if (phase == BrowserPhase.NeedsYou) copy(phase = BrowserPhase.Waiting) else this
    fun takeOver() = if (phase in setOf(BrowserPhase.NeedsYou, BrowserPhase.Waiting)) copy(phase = BrowserPhase.Driving) else this
    fun text(chars: Int) = if (phase == BrowserPhase.Driving) copy(typed = (typed + chars).coerceIn(0, 1024)) else this
    fun backspace() = if (phase == BrowserPhase.Driving) copy(typed = (typed - 1).coerceAtLeast(0)) else this
    fun done() = if (phase == BrowserPhase.Driving && typed > 0) copy(phase = BrowserPhase.Finished, typed = 0) else this
}

data class BrowserSession(val spaceId: String, val agent: Persona, val site: String, val locale: String)
data class BrowserState(val session: BrowserSession? = null, val ownership: BrowserOwnership = BrowserOwnership(), val frame: Bitmap? = null, val failure: Boolean = false) {
    val phase get() = ownership.phase
}

/** The demo transport runs the VM half locally, using the production Rust live-view crypto. */
internal class DemoLiveViewChannel(key: LiveViewKey) : AutoCloseable {
    val vm = LiveViewDemoVm.start(key.publicKey())
    val live: LiveViewSession = key.accept(vm.vmPub(), vm.session())
    fun frame(jpeg: ByteArray): ByteArray = live.openFrame(vm.sealFrame(jpeg))
    fun text(value: String): LiveViewInput = vm.openInput(live.sealText(value))
    fun key(value: String): LiveViewInput = vm.openInput(live.sealKey(value))
    fun click(x: Double, y: Double): LiveViewInput = vm.openInput(live.sealClick(x, y))
    fun done(): LiveViewInput = vm.openInput(live.sealDone())
    override fun close() { live.destroy(); vm.destroy() }
}

class AgentBrowser(private val context: Context, private val scope: CoroutineScope, private val vault: AndroidSecretVault) : AutoCloseable {
    private val mutableState = MutableStateFlow(BrowserState())
    val state = mutableState.asStateFlow()
    private var channel: DemoLiveViewChannel? = null
    private var loop: Job? = null
    private var step = 0

    fun startDemo(spaceId: String, agent: Persona, locale: String) {
        check(BuildConfig.DEBUG) { "The simulated browser is available in debug builds only." }
        loop?.cancel()
        channel?.close()
        mutableState.value = BrowserState(session = BrowserSession(spaceId, agent, "casaazulparaty.com.br", locale))
        step = 0
        try {
            val secret = vault.load("liveview-device")
            try {
                val key = if (secret == null) LiveViewKey.generate().also {
                    val bytes = it.secret()
                    try { check(vault.save("liveview-device", bytes)) { "Couldn't keep the device key." } } finally { bytes.fill(0) }
                } else LiveViewKey.restore(secret)
                try { channel = DemoLiveViewChannel(key) } finally { key.destroy() }
            } finally { secret?.fill(0) }
            push()
            loop = scope.launch {
                for (next in 1..3) { delay(1600); step = next; push() }
                mutableState.value = state.value.copy(ownership = state.value.ownership.needsYou())
            }
        } catch (_: Exception) { fail() }
    }

    fun takeOver() { if (!state.value.failure) mutableState.value = state.value.copy(ownership = state.value.ownership.takeOver()) }
    fun notNow() { mutableState.value = state.value.copy(ownership = state.value.ownership.notNow()) }

    fun typeChanged(old: String, new: String) {
        if (state.value.failure || state.value.phase != BrowserPhase.Driving || new.codePointCount(0, new.length) > 1024) return
        val transport = channel ?: return
        try {
            val before = old.codePoints().toArray()
            val after = new.codePoints().toArray()
            try {
                var prefix = 0
                while (prefix < before.size && prefix < after.size && before[prefix] == after[prefix]) prefix++
                repeat(before.size - prefix) { apply(transport.key("Backspace")) }
                if (after.size > prefix) apply(transport.text(String(after, prefix, after.size - prefix)))
            } finally { before.fill(0); after.fill(0) }
            push()
        } catch (_: Exception) { fail() }
    }

    fun click(x: Double, y: Double) {
        if (state.value.phase != BrowserPhase.Driving || x !in 0.0..360.0 || y !in 0.0..480.0) return
        try { channel?.let { apply(it.click(x, y)) } } catch (_: Exception) { fail() }
    }

    fun done() {
        if (state.value.failure || state.value.phase != BrowserPhase.Driving || state.value.ownership.typed == 0) return
        try { channel?.let { apply(it.done()); step = 4; push() } } catch (_: Exception) { fail() }
    }

    /** Hide the owner's input without handing control to the agent. */
    fun clearTypedInput() {
        if (state.value.phase != BrowserPhase.Driving) return
        try {
            channel?.let { transport -> repeat(state.value.ownership.typed) { apply(transport.key("Backspace")) } }
            push()
        } catch (_: Exception) { fail() }
    }

    private fun apply(input: LiveViewInput) {
        val current = state.value.ownership
        val next = when (input) {
            is LiveViewInput.Text -> current.text(input.chars.toInt())
            is LiveViewInput.Key -> if (input.key == "Backspace") current.backspace() else current
            is LiveViewInput.Click -> current
            LiveViewInput.Done -> current.done()
        }
        mutableState.value = state.value.copy(ownership = next)
    }

    private fun push() {
        val session = state.value.session ?: return
        val drawn = DemoSiteRenderer.draw(step, state.value.ownership.typed, session.locale)
        val jpeg = ByteArrayOutputStream().use { output -> drawn.compress(Bitmap.CompressFormat.JPEG, 72, output); output.toByteArray() }
        drawn.recycle()
        val opened = try { channel?.frame(jpeg) ?: return } finally { jpeg.fill(0) }
        try {
            val frame = checkNotNull(BitmapFactory.decodeByteArray(opened, 0, opened.size))
            mutableState.value = state.value.copy(frame = frame)
        } finally { opened.fill(0) }
    }

    private fun fail() { loop?.cancel(); channel?.close(); channel = null; mutableState.value = state.value.copy(failure = true) }
    fun clear() { loop?.cancel(); channel?.close(); channel = null; mutableState.value = BrowserState() }
    override fun close() = clear()
}

private object DemoSiteRenderer {
    fun draw(step: Int, dots: Int, locale: String): Bitmap {
        val bitmap = Bitmap.createBitmap(720, 960, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap).apply { scale(2f, 2f); drawColor(Color.WHITE) }
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        val blue = Color.rgb(33, 92, 171)
        fun text(value: String, x: Float, y: Float, size: Float = 14f, color: Int = Color.rgb(30, 30, 30), bold: Boolean = false) {
            paint.color = color; paint.shader = null; paint.textSize = size; paint.typeface = if (bold) Typeface.create("sans-serif", Typeface.BOLD) else Typeface.create("sans-serif", Typeface.NORMAL)
            canvas.drawText(value, x, y, paint)
        }
        text("Casa Azul", 18f, 33f, 22f, blue, true)
        text("Paraty · RJ", 272f, 30f, 12f, Color.GRAY)
        paint.shader = LinearGradient(0f, 52f, 0f, 180f, Color.rgb(140, 205, 240), Color.rgb(46, 140, 191), Shader.TileMode.CLAMP)
        val bannerBottom = if (step >= 3) 122f else 182f
        canvas.drawRect(0f, 52f, 360f, bannerBottom, paint)
        text(pick(locale, if (step >= 3) "Confirm booking" else "An inn in the historic center", if (step >= 3) "Confirmar reserva" else "Pousada no centro histórico"), 18f, bannerBottom - 18, 17f, Color.WHITE, true)
        var y = bannerBottom + 34
        fun row(label: String, value: String) {
            text(label, 18f, y, 12f, Color.GRAY)
            paint.textSize = 13f
            text(value, (342f - paint.measureText(value)).coerceAtLeast(105f), y, 13f)
            paint.color = Color.rgb(230, 230, 230); canvas.drawRect(18f, y + 10, 342f, y + 11, paint); y += 42
        }
        row(pick(locale, "Dates", "Datas"), if (step >= 1) pick(locale, "Fri 17–Sun 19 Oct", "sex 17–dom 19 out") else pick(locale, "Choose dates", "Escolha as datas"))
        row(pick(locale, "Guests", "Hóspedes"), pick(locale, "2 adults", "2 adultos"))
        row(pick(locale, "Room", "Quarto"), if (step >= 2) pick(locale, "Garden · breakfast · R$640", "Jardim · café incluso · R$640") else "—")
        if (step == 3) {
            text(pick(locale, "Sign in to confirm", "Entre para confirmar"), 18f, y, 15f, bold = true); y += 22
            for ((label, value) in listOf(pick(locale, "Email", "E-mail") to "demo@example.com", pick(locale, "Password", "Senha") to "•".repeat(dots.coerceAtMost(18)))) {
                text(label, 18f, y, 11f, Color.GRAY); y += 8
                paint.color = if (label == pick(locale, "Password", "Senha")) blue else Color.LTGRAY; paint.style = Paint.Style.STROKE; paint.strokeWidth = 1f
                canvas.drawRoundRect(18f, y, 342f, y + 34, 8f, 8f, paint); paint.style = Paint.Style.FILL
                text(value, 28f, y + 23, 14f); y += 52
            }
        }
        if (step >= 4) text(pick(locale, "Demo confirmed · no. 48213", "Demo confirmada · nº 48213"), 18f, y + 10, 15f, Color.rgb(30, 130, 70), true)
        else {
            paint.color = blue; canvas.drawRoundRect(18f, 427f, 342f, 467f, 10f, 10f, paint)
            text(pick(locale, if (step >= 3) "Sign in" else "Continue", if (step >= 3) "Entrar" else "Continuar"), 145f, 453f, 15f, Color.WHITE, true)
        }
        return bitmap
    }
}
