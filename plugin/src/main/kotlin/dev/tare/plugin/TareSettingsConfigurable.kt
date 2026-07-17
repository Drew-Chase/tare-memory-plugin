package dev.tare.plugin

import com.intellij.openapi.options.Configurable
import com.intellij.openapi.project.Project
import javax.swing.*

/**
 * Settings panel under Tools → Tare Allocation Viewer.
 */
class TareSettingsConfigurable(private val project: Project) : Configurable {
    private var enabledCheckbox: JCheckBox? = null
    private var minBytesField: JTextField? = null
    private var metricCombo: JComboBox<TareSettings.Metric>? = null

    override fun getDisplayName(): String = "Tare Allocation Viewer"

    override fun createComponent(): JComponent {
        val settings = TareSettings.getInstance(project)
        val panel = JPanel()
        panel.layout = BoxLayout(panel, BoxLayout.Y_AXIS)

        enabledCheckbox = JCheckBox("Enable Tare allocation hints", settings.enabled)
        panel.add(enabledCheckbox)
        panel.add(Box.createVerticalStrut(8))

        val minBytesPanel = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.X_AXIS)
            add(JLabel("Minimum bytes to show runtime hints: "))
            minBytesField = JTextField(settings.minBytes.toString(), 10)
            add(minBytesField)
        }
        panel.add(minBytesPanel)
        panel.add(Box.createVerticalStrut(8))

        val metricPanel = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.X_AXIS)
            add(JLabel("Display metric: "))
            metricCombo = JComboBox(TareSettings.Metric.entries.toTypedArray())
            metricCombo!!.selectedItem = settings.metric
            add(metricCombo)
        }
        panel.add(metricPanel)

        return panel
    }

    override fun isModified(): Boolean {
        val settings = TareSettings.getInstance(project)
        return enabledCheckbox?.isSelected != settings.enabled
                || minBytesField?.text?.toLongOrNull() != settings.minBytes
                || metricCombo?.selectedItem != settings.metric
    }

    override fun apply() {
        val settings = TareSettings.getInstance(project)
        val state = settings.state
        state.enabled = enabledCheckbox?.isSelected ?: true
        state.minBytes = minBytesField?.text?.toLongOrNull() ?: 0
        state.metric = metricCombo?.selectedItem as? TareSettings.Metric
            ?: TareSettings.Metric.CUMULATIVE
        settings.loadState(state)

        // Refresh hints.
        TareDataService.getInstance(project).refresh()
    }

    override fun reset() {
        val settings = TareSettings.getInstance(project)
        enabledCheckbox?.isSelected = settings.enabled
        minBytesField?.text = settings.minBytes.toString()
        metricCombo?.selectedItem = settings.metric
    }
}
