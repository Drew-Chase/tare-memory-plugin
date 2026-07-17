package dev.tare.plugin

import com.intellij.codeInsight.daemon.LineMarkerInfo
import com.intellij.codeInsight.daemon.LineMarkerProvider
import com.intellij.icons.AllIcons
import com.intellij.openapi.editor.markup.GutterIconRenderer
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import javax.swing.Icon

/**
 * Gutter icons on lines with allocation data.
 *
 * Tooltip shows the byte breakdown + top call stacks.
 * Icon varies by data source: runtime, static, or both.
 */
class TareLineMarkerProvider : LineMarkerProvider {

    override fun getLineMarkerInfo(element: PsiElement): LineMarkerInfo<*>? = null

    override fun collectSlowLineMarkers(
        elements: List<PsiElement>,
        result: MutableCollection<in LineMarkerInfo<*>>
    ) {
        if (elements.isEmpty()) return

        val firstElement = elements.first()
        val project = firstElement.project
        val settings = TareSettings.getInstance(project)
        if (!settings.enabled) return

        val service = TareDataService.getInstance(project)
        if (service.report == null) return

        val file = firstElement.containingFile ?: return
        val vf = file.virtualFile ?: return
        val relativePath = service.relativePath(vf.path) ?: return
        val fileData = service.getFileData(relativePath) ?: return

        val document = file.viewProvider.document ?: return
        val isFresh = service.isFileFresh(relativePath, document)

        // Collect the first PsiElement on each line that has allocation data.
        val processedLines = mutableSetOf<Int>()

        for (element in elements) {
            val offset = element.textOffset
            val lineIndex = document.getLineNumber(offset)
            val lineNum = lineIndex + 1 // 1-based

            if (lineNum.toString() !in fileData.lines) continue
            if (!processedLines.add(lineIndex)) continue

            val lineData = fileData.lines[lineNum.toString()] ?: continue

            val hasRuntime = lineData.entries.any { it.isRuntime }
            val hasStatic = lineData.entries.any { it.isStatic }

            val icon = when {
                !isFresh -> AllIcons.General.Warning
                hasRuntime && hasStatic -> AllIcons.Debugger.Db_muted_breakpoint
                hasRuntime -> AllIcons.Actions.ProfileCPU
                hasStatic -> AllIcons.Actions.InlayGlobe
                else -> continue
            }

            val tooltip = buildTooltipHtml(lineData, isFresh)

            result.add(
                LineMarkerInfo(
                    element,
                    element.textRange,
                    icon,
                    { tooltip },
                    null,
                    GutterIconRenderer.Alignment.LEFT
                ) { "Tare: allocation data" }
            )
        }
    }

    private fun buildTooltipHtml(lineData: LineData, fresh: Boolean): String {
        if (!fresh) {
            return "<html><b>Tare:</b> data may be stale — re-run to refresh</html>"
        }

        val sb = StringBuilder("<html><b>Tare Allocation Data</b><br/>")

        for (entry in lineData.entries) {
            when {
                entry.isRuntime && entry.kind == "heap_cumulative" -> {
                    sb.append("<br/><b>Runtime heap:</b><br/>")
                    sb.append("&nbsp;&nbsp;Cumulative: ${TareInlayHintsProvider.formatBytes(entry.bytes ?: 0)}<br/>")
                    sb.append("&nbsp;&nbsp;Allocations: ${entry.count ?: 0}<br/>")
                    sb.append("&nbsp;&nbsp;Peak: ${TareInlayHintsProvider.formatBytes(entry.peakBytes ?: 0)}<br/>")

                    entry.stacks?.take(3)?.forEachIndexed { i, stack ->
                        sb.append("<br/>&nbsp;&nbsp;<i>Stack ${i + 1}</i> (${TareInlayHintsProvider.formatBytes(stack.bytes)}):<br/>")
                        stack.frames.take(5).forEach { frame ->
                            // Truncate long frame names.
                            val display = if (frame.length > 80) frame.take(77) + "..." else frame
                            sb.append("&nbsp;&nbsp;&nbsp;&nbsp;<code>${escapeHtml(display)}</code><br/>")
                        }
                        if (stack.frames.size > 5) {
                            sb.append("&nbsp;&nbsp;&nbsp;&nbsp;<i>... ${stack.frames.size - 5} more frames</i><br/>")
                        }
                    }
                }

                entry.isStatic && entry.kind == "alloc_site" -> {
                    sb.append("<br/><b>Static site:</b> ${entry.constructs?.joinToString(", ") ?: "?"}<br/>")
                    if (entry.amountHint != null) {
                        sb.append("&nbsp;&nbsp;Hint: ${escapeHtml(entry.amountHint)}<br/>")
                    }
                }

                entry.isStatic && entry.kind == "type_size" -> {
                    sb.append("<br/><b>Type size:</b> ${entry.ty} = ${entry.stackBytes}B stack (upper bound)<br/>")
                }
            }
        }

        sb.append("</html>")
        return sb.toString()
    }

    private fun escapeHtml(s: String): String {
        return s.replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
    }
}
