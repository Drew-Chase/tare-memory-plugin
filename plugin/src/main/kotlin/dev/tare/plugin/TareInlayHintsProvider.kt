package dev.tare.plugin

import com.intellij.codeInsight.hints.declarative.*
import com.intellij.openapi.editor.Editor
import com.intellij.psi.PsiFile

/**
 * Declarative inlay hints provider that renders allocation data inline.
 *
 * Runtime entries show solid byte amounts: "384 KiB, 3 allocs"
 * Static entries show site markers: "site: Vec::with_capacity"
 * Stale files show: "⟳ re-run to refresh"
 */
class TareInlayHintsProvider : InlayHintsProvider {

    override fun createCollector(file: PsiFile, editor: Editor): InlayHintsCollector? {
        val project = file.project
        val service = TareDataService.getInstance(project)
        val settings = TareSettings.getInstance(project)

        if (!settings.enabled) return null
        if (service.report == null) return null

        val vf = file.virtualFile ?: return null
        val relativePath = service.relativePath(vf.path) ?: return null
        val fileData = service.getFileData(relativePath) ?: return null

        val document = editor.document
        val isFresh = service.isFileFresh(relativePath, document)

        return TareCollector(fileData, settings, isFresh, document)
    }

    private class TareCollector(
        private val fileData: FileData,
        private val settings: TareSettings,
        private val isFresh: Boolean,
        private val document: com.intellij.openapi.editor.Document
    ) : OwnBypassCollector {

        override fun collectHintsForFile(file: PsiFile, sink: InlayTreeSink) {
            for ((lineStr, lineData) in fileData.lines) {
                val lineNum = lineStr.toIntOrNull() ?: continue
                // Convert 1-based line to 0-based for Document API.
                val lineIndex = lineNum - 1
                if (lineIndex < 0 || lineIndex >= document.lineCount) continue

                val hints = buildHintText(lineData, isFresh)
                if (hints.isEmpty()) continue

                // Place hint at end of line.
                val lineEndOffset = document.getLineEndOffset(lineIndex)

                sink.addPresentation(
                    InlineInlayPosition(lineEndOffset, relatedToPrevious = true),
                    hasBackground = true
                ) {
                    text(hints)
                }
            }
        }

        private fun buildHintText(lineData: LineData, fresh: Boolean): String {
            if (!fresh) return "\u27F3 re-run to refresh"

            val parts = mutableListOf<String>()

            for (entry in lineData.entries) {
                if (entry.isRuntime && entry.kind == "heap_cumulative") {
                    val displayValue = when (settings.metric) {
                        TareSettings.Metric.CUMULATIVE -> entry.bytes
                        TareSettings.Metric.PEAK -> entry.peakBytes
                        TareSettings.Metric.COUNT -> entry.count
                    }

                    if (displayValue != null && displayValue >= settings.minBytes) {
                        when (settings.metric) {
                            TareSettings.Metric.CUMULATIVE ->
                                parts.add("${formatBytes(displayValue)}, ${entry.count} allocs")
                            TareSettings.Metric.PEAK ->
                                parts.add("peak ${formatBytes(displayValue)}")
                            TareSettings.Metric.COUNT ->
                                parts.add("${displayValue} allocs")
                        }
                    }
                }

                if (entry.isStatic && entry.kind == "alloc_site") {
                    val constructs = entry.constructs?.joinToString(", ") ?: continue
                    val hint = if (entry.amountHint != null) {
                        "site: $constructs (${entry.amountHint})"
                    } else {
                        "site: $constructs"
                    }
                    parts.add(hint)
                }

                if (entry.isStatic && entry.kind == "type_size") {
                    if (entry.ty != null && entry.stackBytes != null) {
                        parts.add("${entry.ty}: ${entry.stackBytes}B stack (upper bound)")
                    }
                }
            }

            return parts.joinToString(" | ")
        }
    }

    companion object {
        fun formatBytes(bytes: Long): String {
            return when {
                bytes >= 1_048_576 -> "%.1f MiB".format(bytes.toDouble() / 1_048_576)
                bytes >= 1_024 -> "%.1f KiB".format(bytes.toDouble() / 1_024)
                else -> "$bytes B"
            }
        }
    }
}
