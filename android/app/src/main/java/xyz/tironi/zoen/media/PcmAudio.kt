package xyz.tironi.zoen.media

import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMuxer
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

data class PcmAudio(val samples: ShortArray, val sampleRate: Int, val channels: Int) {
    init { require(sampleRate > 0 && channels in 1..8 && samples.size % channels == 0) }
    val frames get() = samples.size / channels
    val duration get() = frames.toDouble() / sampleRate
    fun waveform(count: Int): List<Float> {
        if (frames == 0) return List(count) { .05f }
        return List(count) { index ->
            val start = (index.toLong() * frames / count).toInt()
            val end = max(start + 1, ((index + 1L) * frames / count).toInt()).coerceAtMost(frames)
            var peak = 0
            for (sample in start * channels until end * channels) peak = max(peak, abs(samples[sample].toInt()))
            (.06f + sqrt(peak / 32768f) * .94f).coerceAtMost(1f)
        }
    }
    fun edit(ranges: List<TimeRange>, fadeSeconds: Double = .015): PcmAudio {
        val frameRanges = ranges.map { range ->
            val start = (range.start * sampleRate).toInt().coerceIn(0, frames)
            val end = (range.end * sampleRate).toInt().coerceIn(start, frames)
            start until end
        }.filterNot { it.isEmpty() }
        val output = ShortArray(frameRanges.sumOf { it.last - it.first + 1 } * channels)
        var cursor = 0
        for (range in frameRanges) {
            val count = range.last - range.first + 1
            val fade = min((fadeSeconds * sampleRate).toInt(), count / 2)
            range.forEachIndexed { index, frame ->
                val factor = if (fade == 0) 1.0 else min(1.0, min(index.toDouble() / fade, (count - 1 - index).toDouble() / fade))
                for (channel in 0 until channels) output[cursor++] = (samples[frame * channels + channel] * factor).toInt().toShort()
            }
        }
        return PcmAudio(output, sampleRate, channels)
    }
    fun monoAt(rate: Int): PcmAudio {
        val length = (duration * rate).toInt()
        val output = ShortArray(length)
        for (i in output.indices) {
            val source = (i.toLong() * sampleRate / rate).toInt().coerceAtMost(max(0, frames - 1))
            var sample = 0
            for (channel in 0 until channels) sample += samples[source * channels + channel]
            output[i] = (sample / channels).toShort()
        }
        return PcmAudio(output, rate, 1)
    }
    fun bytes(): ByteArray = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN).apply { asShortBuffer().put(samples) }.array()

    /** A timed recognizer supplies word starts. Silence in the recording supplies the ends. */
    fun wordEnd(start: Double, next: Double): Double {
        val window = max(1, sampleRate / 50)
        val from = (start * sampleRate).toInt().coerceIn(0, frames)
        val to = (next * sampleRate).toInt().coerceIn(from, frames)
        var lastSpeech = from
        var quietFrames = 0
        for (frame in from until to step window) {
            var energy = 0.0
            val end = min(frame + window, to)
            for (i in frame * channels until end * channels) energy += samples[i].toDouble() * samples[i]
            val rms = sqrt(energy / max(1, (end - frame) * channels))
            if (rms > 300) { lastSpeech = end; quietFrames = 0 } else quietFrames += end - frame
            if (quietFrames > sampleRate * .15 && lastSpeech > from) return (lastSpeech.toDouble() / sampleRate + .02).coerceAtMost(next)
        }
        return max(start + .04, lastSpeech.toDouble() / sampleRate + .02).coerceAtMost(next)
    }
}

object AudioFiles {
    suspend fun decode(file: File): PcmAudio = withContext(Dispatchers.IO) {
        val extractor = MediaExtractor()
        var decoder: MediaCodec? = null
        try {
            extractor.setDataSource(file.absolutePath)
            val track = (0 until extractor.trackCount).firstOrNull { extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true }
                ?: error("This file has no playable audio track.")
            extractor.selectTrack(track)
            val format = extractor.getTrackFormat(track)
            var rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
            var channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
            var encoding = AudioFormat.ENCODING_PCM_16BIT
            decoder = MediaCodec.createDecoderByType(checkNotNull(format.getString(MediaFormat.KEY_MIME)))
            decoder.configure(format, null, null, 0)
            decoder.start()
            val output = ByteArrayOutputStream()
            val info = MediaCodec.BufferInfo()
            var inputDone = false
            var outputDone = false
            val deadline = System.nanoTime() + 180_000_000_000L
            while (!outputDone) {
                coroutineContext.ensureActive()
                check(System.nanoTime() < deadline) { "Audio decoding timed out." }
                if (!inputDone) {
                    val index = decoder.dequeueInputBuffer(10_000)
                    if (index >= 0) {
                        val buffer = checkNotNull(decoder.getInputBuffer(index))
                        val size = extractor.readSampleData(buffer, 0)
                        if (size < 0) { decoder.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inputDone = true }
                        else { decoder.queueInputBuffer(index, 0, size, extractor.sampleTime, 0); extractor.advance() }
                    }
                }
                when (val index = decoder.dequeueOutputBuffer(info, 10_000)) {
                    MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        val decoded = decoder.outputFormat
                        rate = decoded.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                        channels = decoded.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                        encoding = if (decoded.containsKey(MediaFormat.KEY_PCM_ENCODING)) decoded.getInteger(MediaFormat.KEY_PCM_ENCODING) else AudioFormat.ENCODING_PCM_16BIT
                    }
                    else -> if (index >= 0) {
                        if (info.size > 0) {
                            val buffer = checkNotNull(decoder.getOutputBuffer(index)).duplicate().order(ByteOrder.LITTLE_ENDIAN)
                            buffer.position(info.offset); buffer.limit(info.offset + info.size)
                            if (encoding == AudioFormat.ENCODING_PCM_FLOAT) {
                                while (buffer.remaining() >= 4) {
                                    val sample = (buffer.float.coerceIn(-1f, 1f) * 32767).toInt()
                                    output.write(sample and 255); output.write(sample shr 8 and 255)
                                }
                            } else {
                                check(encoding == AudioFormat.ENCODING_PCM_16BIT) { "Unsupported decoded audio encoding." }
                                val bytes = ByteArray(buffer.remaining()); buffer.get(bytes); output.write(bytes)
                            }
                            require(output.size() <= 128 * 1024 * 1024) { "This recording is too long to edit on this device." }
                        }
                        outputDone = info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                        decoder.releaseOutputBuffer(index, false)
                    }
                }
            }
            val bytes = output.toByteArray()
            val samples = ShortArray(bytes.size / 2)
            ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(samples)
            PcmAudio(samples, rate, channels)
        } finally {
            runCatching { decoder?.stop() }; decoder?.release(); extractor.release()
        }
    }

    suspend fun encode(audio: PcmAudio, destination: File): File = withContext(Dispatchers.IO) {
        require(audio.frames > 0)
        destination.parentFile?.mkdirs()
        val codec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_AAC)
        var muxer: MediaMuxer? = null
        var started = false
        var success = false
        try {
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, audio.sampleRate, audio.channels).apply {
                setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
                setInteger(MediaFormat.KEY_BIT_RATE, if (audio.channels == 1) 96_000 else 160_000)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 16_384)
            }
            codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            codec.start()
            muxer = MediaMuxer(destination.absolutePath, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
            var track = -1
            var cursor = 0
            var inputDone = false
            var outputDone = false
            val info = MediaCodec.BufferInfo()
            val deadline = System.nanoTime() + 180_000_000_000L
            while (!outputDone) {
                coroutineContext.ensureActive()
                check(System.nanoTime() < deadline) { "Audio export timed out." }
                if (!inputDone) {
                    val index = codec.dequeueInputBuffer(10_000)
                    if (index >= 0) {
                        val buffer = checkNotNull(codec.getInputBuffer(index)).apply { clear(); order(ByteOrder.LITTLE_ENDIAN) }
                        val count = minOf(buffer.remaining() / (2 * audio.channels), (audio.samples.size - cursor) / audio.channels, 1024) * audio.channels
                        val presentation = (cursor / audio.channels).toLong() * 1_000_000 / audio.sampleRate
                        if (count == 0) { codec.queueInputBuffer(index, 0, 0, presentation, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inputDone = true }
                        else {
                            for (sample in cursor until cursor + count) buffer.putShort(audio.samples[sample])
                            codec.queueInputBuffer(index, 0, count * 2, presentation, 0); cursor += count
                        }
                    }
                }
                when (val index = codec.dequeueOutputBuffer(info, 10_000)) {
                    MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        check(!started)
                        track = muxer.addTrack(codec.outputFormat); muxer.start(); started = true
                    }
                    else -> if (index >= 0) {
                        if (info.size > 0 && info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0) {
                            check(started)
                            val buffer = checkNotNull(codec.getOutputBuffer(index))
                            buffer.position(info.offset); buffer.limit(info.offset + info.size)
                            muxer.writeSampleData(track, buffer, info)
                        }
                        outputDone = info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                        codec.releaseOutputBuffer(index, false)
                    }
                }
            }
            muxer.stop(); started = false
            success = true
            destination
        } finally {
            runCatching { codec.stop() }; codec.release()
            if (started) runCatching { muxer?.stop() }
            muxer?.release()
            if (!success) destination.delete()
        }
    }
}
