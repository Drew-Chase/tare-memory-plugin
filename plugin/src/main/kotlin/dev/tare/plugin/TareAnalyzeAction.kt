package dev.tare.plugin

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.ui.popup.JBPopupFactory
import com.intellij.openapi.ui.popup.ListPopup
import com.intellij.openapi.ui.popup.PopupStep
import com.intellij.openapi.ui.popup.util.BaseListPopupStep
import java.io.File

/**
 * Toolbar / menu action: "Run Tare Analysis"
 *
 * Shows a popup with three choices:
 * - Static Only (no run, fast)
 * - Profile (runtime, runs the binary)
 * - All (static + runtime, merged)
 *
 * Runs the selected cargo xtask command in the background and
 * refreshes hints on completion.
 */
class TareAnalyzeAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val settings = TareSettings.getInstance(project)

        val choices = listOf(
            AnalysisMode("Static Only", "static",
                "Analyze allocation sites (no run needed, fast)"),
            AnalysisMode("Profile (Runtime)", "profile",
                "Run the binary with dhat to capture real heap data"),
            AnalysisMode("All (Static + Runtime)", "all",
                "Both analyses, merged into one report")
        )

        val popup: ListPopup = JBPopupFactory.getInstance().createListPopup(
            object : BaseListPopupStep<AnalysisMode>("Run Tare Analysis", choices) {
                override fun getTextFor(value: AnalysisMode): String = value.label

                override fun onChosen(selectedValue: AnalysisMode, finalChoice: Boolean): PopupStep<*>? {
                    doFinalStep {
                        runAnalysis(project, selectedValue, settings.crateRoot)
                    }
                    return FINAL_CHOICE
                }
            }
        )

        popup.showInBestPositionFor(e.dataContext)
    }

    override fun update(e: AnActionEvent) {
        val project = e.project
        // Only enabled for projects that look like Rust projects.
        e.presentation.isEnabledAndVisible = project != null
                && project.basePath != null
                && File(project.basePath!!, "Cargo.toml").exists()
    }

    private fun runAnalysis(
        project: com.intellij.openapi.project.Project,
        mode: AnalysisMode,
        crateRoot: String
    ) {
        notify(project, "Running: ${mode.label}...", NotificationType.INFORMATION)

        TareProcessRunner.runXtask(
            project = project,
            command = mode.command,
            crateRoot = crateRoot,
            title = "Tare: ${mode.label}",
            onSuccess = {
                TareDataService.getInstance(project).refresh()
                notify(project, "${mode.label} complete", NotificationType.INFORMATION)
            },
            onFailure = { exitCode ->
                notify(
                    project,
                    "${mode.label} failed (exit $exitCode). Check the event log for details.",
                    NotificationType.WARNING
                )
            }
        )
    }

    private fun notify(
        project: com.intellij.openapi.project.Project,
        content: String,
        type: NotificationType
    ) {
        try {
            NotificationGroupManager.getInstance()
                .getNotificationGroup("Tare")
                .createNotification(content, type)
                .notify(project)
        } catch (_: Exception) {
            // Notification group may not be registered.
        }
    }

    private data class AnalysisMode(
        val label: String,
        val command: String,
        val description: String
    )
}
