package xyz.tironi.zoen.agent

import com.google.mlkit.genai.common.DownloadStatus
import com.google.mlkit.genai.common.FeatureStatus
import com.google.mlkit.genai.prompt.Generation
import com.google.mlkit.genai.prompt.TextPart
import com.google.mlkit.genai.prompt.generateContentRequest
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

enum class ModelStatus { Checking, Ready, Downloadable, Downloading, Unavailable, Failed }

data class ModelAvailability(val status: ModelStatus, val downloadedBytes: Long = 0)

interface OnDeviceModel : AutoCloseable {
    suspend fun check(): ModelAvailability
    suspend fun generate(prompt: String, maxTokens: Int): String
    fun download(): Flow<ModelAvailability>
}

/** AICore runs Gemini Nano locally. No prompt is sent to a hosted model. */
class GeminiNanoModel : OnDeviceModel {
    private val engine = lazy { Generation.getClient() }
    private val model get() = engine.value

    override suspend fun check() = ModelAvailability(when (model.checkStatus()) {
        FeatureStatus.AVAILABLE -> ModelStatus.Ready
        FeatureStatus.DOWNLOADABLE -> ModelStatus.Downloadable
        FeatureStatus.DOWNLOADING -> ModelStatus.Downloading
        else -> ModelStatus.Unavailable
    })

    override suspend fun generate(prompt: String, maxTokens: Int): String {
        require(prompt.length <= 12_000)
        val request = generateContentRequest(TextPart(prompt)) {
            temperature = 0.2f
            maxOutputTokens = maxTokens.coerceIn(1, 2048)
            candidateCount = 1
        }
        return model.generateContent(request).candidates.firstOrNull()?.text?.trim().orEmpty()
    }

    override fun download(): Flow<ModelAvailability> = flow {
        model.download().collect { status ->
            emit(when (status) {
                is DownloadStatus.DownloadStarted -> ModelAvailability(ModelStatus.Downloading)
                is DownloadStatus.DownloadProgress -> ModelAvailability(ModelStatus.Downloading, status.totalBytesDownloaded)
                DownloadStatus.DownloadCompleted -> check()
                is DownloadStatus.DownloadFailed -> ModelAvailability(ModelStatus.Failed)
            })
        }
    }

    override fun close() { if (engine.isInitialized()) model.close() }
}
