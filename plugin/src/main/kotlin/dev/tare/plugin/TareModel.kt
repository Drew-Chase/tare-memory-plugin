package dev.tare.plugin

import com.google.gson.Gson
import com.google.gson.annotations.SerializedName

/**
 * Kotlin mirror of the tare-schema JSON contract.
 * Parsed from target/tare/allocations.json.
 */
data class AllocationReport(
    val version: Int,
    @SerializedName("generated_at") val generatedAt: String,
    @SerializedName("workspace_root") val workspaceRoot: String,
    val files: Map<String, FileData>
)

data class FileData(
    @SerializedName("content_hash") val contentHash: String,
    val lines: Map<String, LineData>
)

data class LineData(
    val entries: List<Entry>
)

data class Entry(
    val source: String,        // "runtime" | "static"
    val kind: String,          // "heap_cumulative" | "alloc_site" | "type_size"
    val bytes: Long? = null,
    val count: Long? = null,
    @SerializedName("peak_bytes") val peakBytes: Long? = null,
    val stacks: List<StackTrace>? = null,
    val constructs: List<String>? = null,
    @SerializedName("amount_hint") val amountHint: String? = null,
    val ty: String? = null,
    @SerializedName("stack_bytes") val stackBytes: Long? = null
) {
    val isRuntime get() = source == "runtime"
    val isStatic get() = source == "static"
}

data class StackTrace(
    val frames: List<String>,
    val bytes: Long
)

object TareJson {
    private val gson = Gson()

    fun parse(json: String): AllocationReport? {
        return try {
            gson.fromJson(json, AllocationReport::class.java)
        } catch (e: Exception) {
            null
        }
    }
}
