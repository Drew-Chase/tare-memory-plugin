package dev.tare.plugin

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.process.OSProcessHandler
import com.intellij.execution.process.ProcessAdapter
import com.intellij.execution.process.ProcessEvent
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.startup.ProjectActivity
import java.io.File
import java.nio.file.Path

/**
 * Runs static analysis automatically when a Rust project is opened.
 *
 * Static analysis is safe (no execution, just syn parsing), fast, and
 * requires no user setup — so it runs on every project open by default.
 * The user can disable this in Settings > Tools > Tare.
 *
 * Runtime profiling is never automatic — it requires deliberate setup
 * (tare-profile feature + dhat) and executes the user's binary.
 */
class TareStartupActivity : ProjectActivity {
    private val log = Logger.getInstance(TareStartupActivity::class.java)

    override suspend fun execute(project: Project) {
        val settings = TareSettings.getInstance(project)
        if (!settings.enabled || !settings.autoRunStatic) return

        val basePath = project.basePath ?: return

        // Only run for Rust projects.
        if (!File(basePath, "Cargo.toml").exists()) return

        // Skip if a recent report already exists (< 5 minutes old).
        val existing = Path.of(basePath, "target", "tare", "allocations.json").toFile()
        if (existing.exists()) {
            val ageMs = System.currentTimeMillis() - existing.lastModified()
            if (ageMs < 5 * 60 * 1000) {
                log.info("Tare: recent report exists, skipping auto-analysis")
                TareDataService.getInstance(project).refresh()
                return
            }
        }

        TareProcessRunner.runXtask(
            project = project,
            command = "static",
            crateRoot = settings.crateRoot,
            title = "Tare: static analysis",
            onSuccess = {
                TareDataService.getInstance(project).refresh()
                notify(project, "Static analysis complete", NotificationType.INFORMATION)
            },
            onFailure = { exitCode ->
                log.warn("Tare: static analysis failed (exit $exitCode)")
            }
        )
    }

    private fun notify(project: Project, content: String, type: NotificationType) {
        ApplicationManager.getApplication().invokeLater {
            try {
                NotificationGroupManager.getInstance()
                    .getNotificationGroup("Tare")
                    .createNotification(content, type)
                    .notify(project)
            } catch (_: Exception) {
                // Notification group not registered yet — not critical.
                log.info("Tare: $content")
            }
        }
    }
}
