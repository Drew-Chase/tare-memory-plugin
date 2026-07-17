package dev.tare.plugin

import com.intellij.openapi.components.*
import com.intellij.openapi.project.Project

/**
 * Persistent settings for the Tare plugin, scoped per project.
 */
@Service(Service.Level.PROJECT)
@State(
    name = "TareSettings",
    storages = [Storage("tare.xml")]
)
class TareSettings : PersistentStateComponent<TareSettings.State> {
    data class State(
        var enabled: Boolean = true,
        var minBytes: Long = 0,
        var metric: Metric = Metric.CUMULATIVE,
        var autoRunStatic: Boolean = true,
        var crateRoot: String = "."
    )

    enum class Metric {
        CUMULATIVE,
        PEAK,
        COUNT
    }

    private var myState = State()

    override fun getState(): State = myState
    override fun loadState(state: State) { myState = state }

    val enabled get() = myState.enabled
    val minBytes get() = myState.minBytes
    val metric get() = myState.metric
    val autoRunStatic get() = myState.autoRunStatic
    val crateRoot get() = myState.crateRoot

    companion object {
        fun getInstance(project: Project): TareSettings =
            project.getService(TareSettings::class.java)
    }
}
