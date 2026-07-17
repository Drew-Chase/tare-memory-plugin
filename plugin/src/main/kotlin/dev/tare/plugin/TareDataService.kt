package dev.tare.plugin

import com.intellij.openapi.Disposable
import com.intellij.openapi.components.Service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.editor.Document
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFileManager
import com.intellij.openapi.vfs.newvfs.BulkFileListener
import com.intellij.openapi.vfs.newvfs.events.VFileEvent
import com.intellij.psi.PsiManager
import com.intellij.codeInsight.daemon.DaemonCodeAnalyzer
import java.io.File
import java.nio.file.Path

/**
 * Project-level service that loads and caches allocation data from
 * target/tare/allocations.json.
 *
 * Watches the JSON file for changes and triggers hint refresh.
 * Checks content hashes to implement hide-on-dirty.
 */
@Service(Service.Level.PROJECT)
class TareDataService(private val project: Project) : Disposable {
    private val log = Logger.getInstance(TareDataService::class.java)

    @Volatile
    var report: AllocationReport? = null
        private set

    init {
        loadReport()
        subscribeToFileChanges()
    }

    fun refresh() {
        loadReport()
        restartAnalysis()
    }

    /**
     * Get allocation data for a file, identified by its path relative
     * to the workspace root (e.g., "src/lib.rs").
     *
     * Returns null if the file has no data or hints are disabled.
     */
    fun getFileData(relativePath: String): FileData? {
        if (!TareSettings.getInstance(project).enabled) return null
        return report?.files?.get(relativePath)
    }

    /**
     * Check if a file's content matches the hash recorded at generation time.
     * Returns false (stale) if the document text's blake3 hash differs.
     */
    fun isFileFresh(relativePath: String, document: Document): Boolean {
        val fileData = report?.files?.get(relativePath) ?: return true
        val currentHash = computeBlake3(document.text)
        return currentHash == fileData.contentHash
    }

    /**
     * Compute the relative path of a file from the workspace root.
     * Returns null if the file is not under the workspace root.
     */
    fun relativePath(absolutePath: String): String? {
        val wsRoot = report?.workspaceRoot ?: return null
        // Normalize both paths.
        val normalizedAbs = absolutePath.replace('\\', '/')
        val normalizedRoot = wsRoot.replace('\\', '/').trimEnd('/')

        // Handle Windows UNC-style paths from canonicalize: //?/D:/...
        val cleanRoot = normalizedRoot
            .removePrefix("//?/")
            .removePrefix("//./")

        val cleanAbs = normalizedAbs
            .removePrefix("//?/")
            .removePrefix("//./")

        return if (cleanAbs.startsWith(cleanRoot)) {
            cleanAbs.removePrefix(cleanRoot).trimStart('/')
        } else {
            null
        }
    }

    private fun loadReport() {
        val jsonPath = findAllocationsJson() ?: return
        try {
            val json = jsonPath.toFile().readText()
            report = TareJson.parse(json)
            if (report != null) {
                log.info("Tare: loaded ${report!!.files.size} files from ${jsonPath}")
            }
        } catch (e: Exception) {
            log.warn("Tare: failed to load allocations.json", e)
            report = null
        }
    }

    private fun findAllocationsJson(): Path? {
        val basePath = project.basePath ?: return null
        val path = Path.of(basePath, "target", "tare", "allocations.json")
        return if (path.toFile().exists()) path else null
    }

    private fun subscribeToFileChanges() {
        project.messageBus.connect(this).subscribe(
            VirtualFileManager.VFS_CHANGES,
            object : BulkFileListener {
                override fun after(events: List<VFileEvent>) {
                    val relevant = events.any { event ->
                        event.path?.endsWith("target/tare/allocations.json") == true
                                || event.path?.endsWith("target\\tare\\allocations.json") == true
                    }
                    if (relevant) {
                        loadReport()
                        restartAnalysis()
                    }
                }
            }
        )
    }

    private fun restartAnalysis() {
        val psiManager = PsiManager.getInstance(project)
        val analyzer = DaemonCodeAnalyzer.getInstance(project)

        // Restart analysis on all open files to refresh hints.
        com.intellij.openapi.fileEditor.FileEditorManager.getInstance(project)
            .openFiles.forEach { vf ->
                psiManager.findFile(vf)?.let { psiFile ->
                    analyzer.restart(psiFile)
                }
            }
    }

    override fun dispose() {
        report = null
    }

    companion object {
        fun getInstance(project: Project): TareDataService =
            project.getService(TareDataService::class.java)

        /**
         * Compute blake3 hex hash of text content, matching the Rust side.
         */
        fun computeBlake3(text: String): String {
            val hasher = io.github.rctcwyvrn.blake3.Blake3.newInstance()
            hasher.update(text.toByteArray(Charsets.UTF_8))
            return hasher.hexdigest()
        }
    }
}
