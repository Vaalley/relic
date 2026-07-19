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
import uniffi.relic_ffi.CollectionInfo
import uniffi.relic_ffi.EventListener
import uniffi.relic_ffi.GameInfo
import uniffi.relic_ffi.GameStatsInfo
import uniffi.relic_ffi.PlayTotals
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
    var search by mutableStateOf("")
        private set
    var favoritesOnly by mutableStateOf(false)
        private set
    var selectedGame by mutableStateOf<GameInfo?>(null)
        private set
    var collections by mutableStateOf<List<CollectionInfo>>(emptyList())
        private set
    var viewingCollections by mutableStateOf(false)
        private set
    var selectedCollection by mutableStateOf<CollectionInfo?>(null)
        private set
    var collectionGames by mutableStateOf<List<GameInfo>>(emptyList())
        private set
    var viewingStats by mutableStateOf(false)
        private set
    var recentlyPlayed by mutableStateOf<List<GameStatsInfo>>(emptyList())
        private set
    var mostPlayed by mutableStateOf<List<GameStatsInfo>>(emptyList())
        private set
    var playTotals by mutableStateOf<PlayTotals?>(null)
        private set

    /** Games as the grid should show them (favorites filter is client-side). */
    val visibleGames: List<GameInfo>
        get() = if (favoritesOnly) games.filter { it.favorite } else games

    fun boxartPath(gameId: Long): String? = engine.boxartPath(gameId)

    fun defaultCore(slug: String): String? = engine.systemDefaultCore(slug)

    fun selectSystem(slug: String?) {
        selectedSystem = slug
        refreshGames()
    }

    fun setSearchQuery(query: String) {
        search = query
        refreshGames()
    }

    fun toggleFavoritesOnly() {
        favoritesOnly = !favoritesOnly
    }

    fun openGame(game: GameInfo) {
        selectedGame = game
    }

    fun closeGame() {
        selectedGame = null
    }

    fun toggleFavorite(game: GameInfo) {
        viewModelScope.launch(Dispatchers.IO) {
            engine.setFavorite(game.id, !game.favorite)
            val g = engine.queryGames(selectedSystem, search.ifBlank { null })
            withContext(Dispatchers.Main) {
                games = g
                if (selectedGame?.id == game.id) {
                    selectedGame = game.copy(favorite = !game.favorite)
                }
            }
        }
    }

    fun refresh() {
        viewModelScope.launch(Dispatchers.IO) {
            val sys = engine.listSystems().filter { it.gameCount > 0 }
            val g = engine.queryGames(selectedSystem, search.ifBlank { null })
            withContext(Dispatchers.Main) {
                systems = sys
                games = g
            }
        }
    }

    private fun refreshGames() {
        viewModelScope.launch(Dispatchers.IO) {
            val g = engine.queryGames(selectedSystem, search.ifBlank { null })
            withContext(Dispatchers.Main) { games = g }
        }
    }

    fun openCollections() {
        viewingCollections = true
        refreshCollections()
    }

    /** Populates [collections] without navigating — used by DetailScreen's quick-add row. */
    fun refreshCollectionsQuietly() {
        refreshCollections()
    }

    fun closeCollections() {
        viewingCollections = false
        selectedCollection = null
    }

    fun openCollection(c: CollectionInfo) {
        selectedCollection = c
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val g = engine.collectionGames(c.id)
                withContext(Dispatchers.Main) { collectionGames = g }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "collections: ${e.message}" }
            }
        }
    }

    fun closeCollectionDetail() {
        selectedCollection = null
    }

    private fun refreshCollections() {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val c = engine.listCollections()
                withContext(Dispatchers.Main) { collections = c }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "collections: ${e.message}" }
            }
        }
    }

    fun createManualCollection(name: String) {
        if (name.isBlank()) return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                engine.createManualCollection(name.trim())
                val c = engine.listCollections()
                withContext(Dispatchers.Main) { collections = c }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "collections: ${e.message}" }
            }
        }
    }

    fun deleteCollection(c: CollectionInfo) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                engine.deleteCollection(c.id)
                val remaining = engine.listCollections()
                withContext(Dispatchers.Main) {
                    collections = remaining
                    if (selectedCollection?.id == c.id) selectedCollection = null
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "collections: ${e.message}" }
            }
        }
    }

    fun addGameToCollection(collectionId: Long, game: GameInfo) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                engine.addToCollection(collectionId, game.id)
                if (selectedCollection?.id == collectionId) {
                    val g = engine.collectionGames(collectionId)
                    withContext(Dispatchers.Main) { collectionGames = g }
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "collections: ${e.message}" }
            }
        }
    }

    fun removeGameFromCollection(collectionId: Long, game: GameInfo) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                engine.removeFromCollection(collectionId, game.id)
                if (selectedCollection?.id == collectionId) {
                    val g = engine.collectionGames(collectionId)
                    withContext(Dispatchers.Main) { collectionGames = g }
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "collections: ${e.message}" }
            }
        }
    }

    fun openStats() {
        viewingStats = true
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val recent = engine.recentlyPlayed(20uL)
                val most = engine.mostPlayed(20uL)
                val totals = engine.playTotals()
                withContext(Dispatchers.Main) {
                    recentlyPlayed = recent
                    mostPlayed = most
                    playTotals = totals
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { status = "stats: ${e.message}" }
            }
        }
    }

    fun closeStats() {
        viewingStats = false
    }

    /**
     * Incremental rescan on activity resume (e.g. returning from a game or a
     * file manager). Quiet: no status noise unless the library changed.
     * Debounced so rapid app switches don't queue scans.
     */
    fun rescanOnResume() {
        val now = System.currentTimeMillis()
        if (scanning || !hasLibrary || now - lastAutoRescan < 15_000) return
        lastAutoRescan = now
        scanLibrary(quiet = true)
    }

    private var lastAutoRescan = 0L

    /** Re-open the setup screen so the library folder can be changed. */
    fun editLibrary() {
        hasLibrary = false
    }

    fun scanLibrary(quiet: Boolean = false) {
        if (scanning) return
        val root = libraryPath.trim().trimEnd('/')
        libraryPath = root
        scanning = true
        if (!quiet) status = null
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
                // A quiet (auto) rescan only speaks up when something changed.
                if (!quiet || summary.added > 0uL || summary.removed > 0uL) {
                    withContext(Dispatchers.Main) { status = message }
                }
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
