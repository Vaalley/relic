package dev.relic.android

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import uniffi.relic_ffi.EventListener
import uniffi.relic_ffi.GameInfo
import uniffi.relic_ffi.RelicEngine
import uniffi.relic_ffi.SystemInfo

/**
 * Bridges the Rust engine (via UniFFI) to Compose state. All engine calls run
 * on IO; the engine object itself is internally synchronized.
 */
class RelicViewModel(app: Application) : AndroidViewModel(app) {

    private val prefs = app.getSharedPreferences("relic", 0)
    private val engine: RelicEngine by lazy {
        RelicEngine.open(File(app.filesDir, "relic.db").absolutePath)
    }

    var libraryPath by mutableStateOf(prefs.getString("library_path", DEFAULT_LIBRARY) ?: DEFAULT_LIBRARY)
    var systems by mutableStateOf<List<SystemInfo>>(emptyList())
        private set
    var games by mutableStateOf<List<GameInfo>>(emptyList())
        private set
    var selectedSystem by mutableStateOf<String?>(null)
        private set
    var scanning by mutableStateOf(false)
        private set
    var progress by mutableStateOf<Pair<ULong, ULong>?>(null)
        private set
    var status by mutableStateOf<String?>(null)
    var hasLibrary by mutableStateOf(prefs.contains("library_path"))
        private set

    fun boxartPath(gameId: Long): String? = engine.boxartPath(gameId)

    fun defaultCore(slug: String): String? = engine.systemDefaultCore(slug)

    fun selectSystem(slug: String?) {
        selectedSystem = slug
        refreshGames()
    }

    fun refresh() {
        viewModelScope.launch(Dispatchers.IO) {
            val sys = engine.listSystems().filter { it.gameCount > 0 }
            val g = engine.queryGames(selectedSystem, null)
            withContext(Dispatchers.Main) {
                systems = sys
                games = g
            }
        }
    }

    private fun refreshGames() {
        viewModelScope.launch(Dispatchers.IO) {
            val g = engine.queryGames(selectedSystem, null)
            withContext(Dispatchers.Main) { games = g }
        }
    }

    /** Re-open the setup screen so the library folder can be changed. */
    fun editLibrary() {
        hasLibrary = false
    }

    fun scanLibrary() {
        if (scanning) return
        val root = libraryPath.trim().trimEnd('/')
        libraryPath = root
        scanning = true
        status = null
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val rootDir = File(root)
                if (!rootDir.isDirectory) {
                    withContext(Dispatchers.Main) { status = "Folder not found: $root" }
                    return@launch
                }
                prefs.edit().putString("library_path", root).apply()
                withContext(Dispatchers.Main) { hasLibrary = true }

                val warnings = mutableListOf<String>()
                val libId = engine.addLibrary(root, rootDir.name.ifEmpty { "library" })
                val summary = engine.scan(libId, object : EventListener {
                    override fun onScanProgress(done: ULong, total: ULong) {
                        viewModelScope.launch { progress = done to total }
                    }
                    override fun onWarning(code: String, context: String) {
                        synchronized(warnings) { warnings.add("$code: $context") }
                    }
                })
                engine.importGamelists(libId)
                engine.refreshMedia(libId)
                val message = buildString {
                    append("added ${summary.added}, removed ${summary.removed}, unchanged ${summary.unchanged}")
                    if (summary.added == 0uL && summary.unchanged == 0uL) {
                        append("\n").append(emptyScanHint(rootDir))
                    }
                    synchronized(warnings) {
                        warnings.filterNot { it.startsWith("scan.no_system_dirs") }
                            .take(3)
                            .forEach { append("\nwarning — ").append(it) }
                    }
                }
                withContext(Dispatchers.Main) { status = message }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "scan failed: ${e.message}" }
            } finally {
                withContext(Dispatchers.Main) {
                    scanning = false
                    progress = null
                }
                refresh()
            }
        }
    }

    /** Why a scan came back empty, in terms the user can act on. */
    private fun emptyScanHint(rootDir: File): String {
        val subdirs = rootDir.listFiles { f: File -> f.isDirectory }?.map { it.name } ?: emptyList()
        val slugs = engine.listSystems().map { it.slug.lowercase() }.toSet()
        val matched = subdirs.filter { it.lowercase() in slugs }
        return when {
            subdirs.isEmpty() ->
                "No subfolders found — Relic expects one folder per system, e.g. ${rootDir.path}/snes"
            matched.isEmpty() ->
                "No folder here matches a system name. Found: ${subdirs.joinToString(", ")}. " +
                    "Rename them after systems (snes, nes, gba, psx, …)"
            else ->
                "Folder(s) ${matched.joinToString(", ")} matched a system but held no ROM files " +
                    "with recognized extensions"
        }
    }

    companion object {
        const val DEFAULT_LIBRARY = "/storage/emulated/0/ROMs"
    }
}
