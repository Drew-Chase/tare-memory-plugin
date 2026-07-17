package dev.tare.plugin

import com.intellij.execution.Executor
import com.intellij.execution.configurations.*
import com.intellij.execution.process.OSProcessHandler
import com.intellij.execution.process.ProcessHandler
import com.intellij.execution.process.ProcessTerminatedListener
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.icons.AllIcons
import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.project.Project
import org.jdom.Element
import javax.swing.*

/**
 * Run configuration type for "Tare Profile" — runs cargo xtask to
 * generate allocation data, then refreshes the plugin's hints.
 */
class TareRunConfigurationType : ConfigurationType {
    override fun getDisplayName(): String = "Tare Profile"
    override fun getConfigurationTypeDescription(): String =
        "Profile Rust allocations with tare and refresh inline hints"
    override fun getIcon(): Icon = AllIcons.Actions.ProfileCPU
    override fun getId(): String = "TareRunConfiguration"
    override fun getConfigurationFactories(): Array<ConfigurationFactory> =
        arrayOf(TareConfigurationFactory(this))
}

class TareConfigurationFactory(type: ConfigurationType) : ConfigurationFactory(type) {
    override fun getId(): String = "TareConfigurationFactory"
    override fun getName(): String = "Tare Profile"

    override fun createTemplateConfiguration(project: Project): RunConfiguration =
        TareRunConfig(project, this, "Tare Profile")
}

class TareRunConfig(
    project: Project,
    factory: ConfigurationFactory,
    name: String
) : RunConfigurationBase<RunConfigurationOptions>(project, factory, name) {

    var command: TareCommand = TareCommand.ALL
    var crateRoot: String = "."
    var extraArgs: String = ""

    enum class TareCommand(val label: String, val arg: String) {
        STATIC("Static only (no run)", "static"),
        PROFILE("Profile (runtime only)", "profile"),
        ALL("All (static + runtime)", "all");

        override fun toString() = label
    }

    override fun getConfigurationEditor(): SettingsEditor<out RunConfiguration> =
        TareRunConfigEditor()

    override fun getState(executor: Executor, env: ExecutionEnvironment): RunProfileState {
        return object : CommandLineState(env) {
            override fun startProcess(): ProcessHandler {
                val cmdLine = GeneralCommandLine("cargo", "xtask", command.arg, crateRoot)

                if (extraArgs.isNotBlank()) {
                    cmdLine.addParameters(extraArgs.split(" ").filter { it.isNotBlank() })
                }

                cmdLine.workDirectory = java.io.File(project.basePath ?: ".")
                cmdLine.environment["TERM"] = "dumb"

                val handler = OSProcessHandler(cmdLine)
                ProcessTerminatedListener.attach(handler)

                // Refresh data after the process finishes.
                handler.addProcessListener(object : com.intellij.execution.process.ProcessAdapter() {
                    override fun processTerminated(event: com.intellij.execution.process.ProcessEvent) {
                        if (event.exitCode == 0) {
                            com.intellij.openapi.application.ApplicationManager.getApplication()
                                .invokeLater {
                                    TareDataService.getInstance(project).refresh()
                                }
                        }
                    }
                })

                return handler
            }
        }
    }

    override fun readExternal(element: Element) {
        super.readExternal(element)
        command = element.getAttributeValue("tare-command")
            ?.let { name -> TareCommand.entries.find { it.arg == name } }
            ?: TareCommand.ALL
        crateRoot = element.getAttributeValue("tare-crate-root") ?: "."
        extraArgs = element.getAttributeValue("tare-extra-args") ?: ""
    }

    override fun writeExternal(element: Element) {
        super.writeExternal(element)
        element.setAttribute("tare-command", command.arg)
        element.setAttribute("tare-crate-root", crateRoot)
        element.setAttribute("tare-extra-args", extraArgs)
    }
}

class TareRunConfigEditor : SettingsEditor<TareRunConfig>() {
    private val commandCombo = JComboBox(TareRunConfig.TareCommand.entries.toTypedArray())
    private val crateRootField = JTextField(".", 30)
    private val extraArgsField = JTextField("", 30)

    override fun createEditor(): JComponent {
        val panel = JPanel()
        panel.layout = BoxLayout(panel, BoxLayout.Y_AXIS)

        panel.add(labeledRow("Command:", commandCombo))
        panel.add(Box.createVerticalStrut(4))
        panel.add(labeledRow("Crate root:", crateRootField))
        panel.add(Box.createVerticalStrut(4))
        panel.add(labeledRow("Extra args:", extraArgsField))

        return panel
    }

    override fun resetEditorFrom(config: TareRunConfig) {
        commandCombo.selectedItem = config.command
        crateRootField.text = config.crateRoot
        extraArgsField.text = config.extraArgs
    }

    override fun applyEditorTo(config: TareRunConfig) {
        config.command = commandCombo.selectedItem as TareRunConfig.TareCommand
        config.crateRoot = crateRootField.text
        config.extraArgs = extraArgsField.text
    }

    private fun labeledRow(label: String, component: JComponent): JPanel {
        return JPanel().apply {
            layout = BoxLayout(this, BoxLayout.X_AXIS)
            add(JLabel(label))
            add(Box.createHorizontalStrut(8))
            add(component)
            add(Box.createHorizontalGlue())
        }
    }
}
