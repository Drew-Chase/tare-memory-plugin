package dev.tare.plugin

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.process.OSProcessHandler
import com.intellij.execution.process.ProcessAdapter
import com.intellij.execution.process.ProcessEvent
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import java.io.File

/**
 * Shared utility for running `cargo xtask` commands from the plugin.
 *
 * Used by both TareStartupActivity (automatic) and TareAnalyzeAction
 * (manual toolbar button).
 */
object TareProcessRunner {
    private val log = Logger.getInstance(TareProcessRunner::class.java)

    /**
     * Run a cargo xtask command in the background.
     *
     * @param command  One of "static", "profile", "all"
     * @param crateRoot  Relative path to the crate root (default ".")
     * @param extraArgs  Additional arguments (e.g., "--bench alloc_bench")
     * @param title  Human-readable description for logging
     * @param onSuccess  Called on EDT when the process exits successfully
     * @param onFailure  Called on EDT when the process exits with error
     */
    fun runXtask(
        project: Project,
        command: String,
        crateRoot: String = ".",
        extraArgs: String = "",
        title: String = "Tare analysis",
        onSuccess: () -> Unit = {},
        onFailure: (exitCode: Int) -> Unit = {}
    ) {
        val basePath = project.basePath ?: return

        // Build the command line.
        val cmdLine = GeneralCommandLine("cargo", "xtask", command, crateRoot)
        if (extraArgs.isNotBlank()) {
            cmdLine.addParameters(extraArgs.split(" ").filter { it.isNotBlank() })
        }
        cmdLine.workDirectory = File(basePath)
        cmdLine.environment["TERM"] = "dumb"

        log.info("$title: cargo xtask $command $crateRoot $extraArgs")

        ApplicationManager.getApplication().executeOnPooledThread {
            try {
                val handler = OSProcessHandler(cmdLine)
                val output = StringBuilder()

                handler.addProcessListener(object : ProcessAdapter() {
                    override fun onTextAvailable(event: ProcessEvent, outputType: com.intellij.openapi.util.Key<*>) {
                        output.append(event.text)
                    }

                    override fun processTerminated(event: ProcessEvent) {
                        ApplicationManager.getApplication().invokeLater {
                            if (event.exitCode == 0) {
                                log.info("$title: completed successfully")
                                onSuccess()
                            } else {
                                log.warn("$title: failed (exit ${event.exitCode})\n$output")
                                onFailure(event.exitCode)
                            }
                        }
                    }
                })

                handler.startNotify()
                handler.waitFor()
            } catch (e: Exception) {
                log.warn("$title: failed to start process", e)
                ApplicationManager.getApplication().invokeLater {
                    onFailure(-1)
                }
            }
        }
    }
}
