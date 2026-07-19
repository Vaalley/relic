package dev.relic.android

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import android.view.KeyEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import uniffi.relic_ffi.GameInfo
import uniffi.relic_ffi.GameStatsInfo
import uniffi.relic_ffi.PendingMatchInfo
import uniffi.relic_ffi.themeColors
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import java.io.File

/**
 * "#rrggbb" (the only format relic-themes emits) → opaque Compose [Color].
 * Never throws, matching relic-themes' "never raises" guarantee.
 */
private fun parseHexColor(hex: String): Color {
    val clean = hex.removePrefix("#")
    return try {
        val rgb = clean.toLong(16).toInt()
        Color(0xFF000000.toInt() or rgb)
    } catch (e: NumberFormatException) {
        Color.Black
    }
}

class MainActivity : ComponentActivity() {

    private val vm: RelicViewModel by viewModels()

    // Compose state, not a plain call: coming back from the Settings grant
    // screen must re-enable the Scan button without any other recomposition.
    private var hasAccess by mutableStateOf(false)

    private val pickFolder =
        registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            if (uri == null) return@registerForActivityResult
            val path = treeUriToPath(uri)
            if (path != null) {
                vm.libraryPath = path
            } else {
                vm.status = "Couldn't turn that folder into a file path — type it manually"
            }
        }

    /**
     * SAF tree URI → plain file path, valid while the app holds all-files
     * access. "primary:ROMs" → /storage/emulated/0/ROMs; "1234-ABCD:ROMs"
     * (SD card) → /storage/1234-ABCD/ROMs. Null if the provider isn't plain
     * external storage or the result doesn't exist on disk.
     */
    private fun treeUriToPath(uri: Uri): String? {
        if (uri.authority != "com.android.externalstorage.documents") return null
        val docId = DocumentsContract.getTreeDocumentId(uri)
        val volume = docId.substringBefore(':')
        val rel = docId.substringAfter(':', "")
        val base =
            if (volume.equals("primary", ignoreCase = true)) {
                Environment.getExternalStorageDirectory().absolutePath
            } else {
                "/storage/$volume"
            }
        val path = if (rel.isEmpty()) base else "$base/$rel"
        return path.takeIf { File(it).isDirectory }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            val tokens = themeColors(true) // dark; light-mode toggle is a later feature
            val colorScheme =
                darkColorScheme(
                    background = parseHexColor(tokens.bg),
                    surface = parseHexColor(tokens.surface),
                    onBackground = parseHexColor(tokens.text),
                    onSurface = parseHexColor(tokens.text),
                    primary = parseHexColor(tokens.accent),
                    secondary = parseHexColor(tokens.favorite),
                )
            MaterialTheme(colorScheme = colorScheme) {
                Scaffold { padding ->
                    Column(Modifier.padding(padding).padding(12.dp)) {
                        val detail = vm.selectedGame
                        when {
                            !vm.hasLibrary -> SetupScreen()
                            detail != null -> DetailScreen(detail)
                            vm.viewingCollections -> CollectionsScreen()
                            vm.viewingStats -> StatsScreen()
                            vm.viewingScraperMatches -> ScraperMatchesScreen()
                            else -> LibraryScreen()
                        }
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        // Relic regaining focus is the only signal available that a game
        // launched via IntentLauncher returned control (docs/android-intents.md
        // §5 step 10) — no per-emulator result contract to rely on instead.
        vm.endPendingSession(this)
        hasAccess = Environment.isExternalStorageManager()
        if (vm.hasLibrary && hasAccess) {
            vm.refresh()
            vm.rescanOnResume()
        }
    }

    /**
     * Gamepad face buttons → the keys Compose's focus system already
     * understands: A confirms (DPAD_CENTER clicks the focused item), B goes
     * back. D-pad focus traversal itself is native Compose behavior.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val mapped = when (event.keyCode) {
            KeyEvent.KEYCODE_BUTTON_A -> KeyEvent.KEYCODE_DPAD_CENTER
            KeyEvent.KEYCODE_BUTTON_B -> KeyEvent.KEYCODE_BACK
            else -> return super.dispatchKeyEvent(event)
        }
        return super.dispatchKeyEvent(KeyEvent(event.action, mapped))
    }

    private fun requestAllFilesAccess() {
        startActivity(
            Intent(
                Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                Uri.parse("package:$packageName"),
            ),
        )
    }

    @Composable
    private fun SetupScreen() {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Welcome to Relic", style = MaterialTheme.typography.headlineMedium)
            Text("Point Relic at your ROM folder (one subfolder per system, e.g. ROMs/snes).")
            OutlinedTextField(
                value = vm.libraryPath,
                onValueChange = { vm.libraryPath = it },
                label = { Text("ROM folder") },
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedButton(onClick = { pickFolder.launch(null) }) {
                Text("Browse…")
            }
            if (!hasAccess) {
                OutlinedButton(onClick = { requestAllFilesAccess() }) {
                    Text("Grant storage access")
                }
                Text(
                    "Relic reads your ROM folder locally; nothing ever leaves the device.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Button(onClick = { vm.scanLibrary() }, enabled = hasAccess && !vm.scanning) {
                Text(if (vm.scanning) "Scanning…" else "Scan library")
            }
            ScanStatus()
        }
    }

    @OptIn(ExperimentalMaterial3Api::class)
    @Composable
    private fun LibraryScreen() {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                item {
                    FilterChip(
                        selected = vm.selectedSystem == null,
                        onClick = { vm.selectSystem(null) },
                        label = { Text("All") },
                    )
                }
                item {
                    FilterChip(
                        selected = vm.favoritesOnly,
                        onClick = { vm.toggleFavoritesOnly() },
                        label = { Text("★ Favorites") },
                    )
                }
                items(vm.systems) { sys ->
                    FilterChip(
                        selected = vm.selectedSystem == sys.slug,
                        onClick = { vm.selectSystem(sys.slug) },
                        label = { Text("${sys.name} (${sys.gameCount})") },
                    )
                }
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                OutlinedTextField(
                    value = vm.search,
                    onValueChange = { vm.setSearchQuery(it) },
                    label = { Text("Search") },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                )
                OutlinedButton(onClick = { vm.scanLibrary() }, enabled = !vm.scanning) {
                    Text(if (vm.scanning) "Scanning…" else "Rescan")
                }
                OutlinedButton(onClick = { vm.editLibrary() }, enabled = !vm.scanning) {
                    Text("Folder")
                }
                OutlinedButton(onClick = { vm.openCollections() }) {
                    Text("Collections")
                }
                OutlinedButton(onClick = { vm.openStats() }) {
                    Text("Stats")
                }
                OutlinedButton(onClick = { vm.openScraperMatches() }) {
                    Text("Scraper")
                }
            }
            ScanStatus()
            LazyVerticalGrid(
                columns = GridCells.Adaptive(120.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(vm.visibleGames, key = { it.id }) { game -> GameTile(game) }
            }
        }
    }

    @Composable
    private fun CollectionsScreen() {
        BackHandler {
            if (vm.selectedCollection != null) vm.closeCollectionDetail() else vm.closeCollections()
        }
        val collection = vm.selectedCollection
        if (collection == null) {
            var newName by remember { mutableStateOf("") }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Collections", style = MaterialTheme.typography.headlineMedium)
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    OutlinedTextField(
                        value = newName,
                        onValueChange = { newName = it },
                        label = { Text("New collection") },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    Button(onClick = {
                        vm.createManualCollection(newName)
                        newName = ""
                    }) {
                        Text("Create")
                    }
                }
                Column(
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()),
                ) {
                    vm.collections.forEach { c ->
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            modifier = Modifier.fillMaxWidth().clickable { vm.openCollection(c) }.padding(8.dp),
                        ) {
                            Text(
                                "${c.name} (${c.kind})",
                                modifier = Modifier.weight(1f),
                            )
                            OutlinedButton(onClick = { vm.deleteCollection(c) }) {
                                Text("Delete")
                            }
                        }
                    }
                }
                OutlinedButton(onClick = { vm.closeCollections() }) {
                    Text("Back")
                }
                vm.status?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(collection.name, style = MaterialTheme.typography.headlineMedium)
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(120.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.fillMaxSize().weight(1f),
                ) {
                    items(vm.collectionGames, key = { it.id }) { game -> GameTile(game) }
                }
                OutlinedButton(onClick = { vm.closeCollectionDetail() }) {
                    Text("Back")
                }
            }
        }
    }

    @Composable
    private fun StatsScreen() {
        BackHandler { vm.closeStats() }
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            Text("Stats", style = MaterialTheme.typography.headlineMedium)
            vm.playTotals?.let {
                Text("${it.sessions} sessions, ${it.totalSeconds / 60}m total")
            }
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()),
            ) {
                Text("Recently Played", style = MaterialTheme.typography.titleMedium)
                vm.recentlyPlayed.forEach { g -> StatsRow(g) }
                Text("Most Played", style = MaterialTheme.typography.titleMedium)
                vm.mostPlayed.forEach { g -> StatsRow(g) }
            }
            OutlinedButton(onClick = { vm.closeStats() }) {
                Text("Back")
            }
        }
    }

    @Composable
    private fun StatsRow(g: GameStatsInfo) {
        Column(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
            Text("${g.name} (${g.systemSlug})", style = MaterialTheme.typography.bodyMedium)
            Text(
                "${g.playCount}x, ${g.totalSeconds / 60}m total, last ${g.lastPlayedAt ?: "-"}",
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }

    @Composable
    private fun ScraperMatchesScreen() {
        BackHandler { vm.closeScraperMatches() }
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            Text("Pending scraper matches", style = MaterialTheme.typography.headlineMedium)
            if (vm.pendingMatches.isEmpty()) {
                Text("No pending matches")
            } else {
                Column(
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()),
                ) {
                    vm.pendingMatches.forEach { match -> PendingMatchRow(match) }
                }
            }
            OutlinedButton(onClick = { vm.closeScraperMatches() }) {
                Text("Back")
            }
        }
    }

    @Composable
    private fun PendingMatchRow(match: PendingMatchInfo) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    vm.pendingMatchGameNames[match.gameId] ?: "game #${match.gameId}",
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    "${match.providerId} — ${match.confidence} confidence — #${match.externalId}",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            OutlinedButton(onClick = { vm.confirmMatch(match.gameId, match.providerId) }) {
                Text("Confirm")
            }
        }
    }

    @Composable
    private fun GameTile(game: GameInfo) {
        // Focus ring for controller navigation: gamepad users need to see
        // where they are; touch users never trigger the focused state.
        val interaction = remember { MutableInteractionSource() }
        val focused by interaction.collectIsFocusedAsState()
        Card(
            onClick = { vm.openGame(game) },
            interactionSource = interaction,
            border = if (focused) BorderStroke(3.dp, MaterialTheme.colorScheme.primary) else null,
        ) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                val art = vm.boxartPath(game.id)
                if (art != null) {
                    AsyncImage(
                        model = File(art),
                        contentDescription = game.name,
                        modifier = Modifier.fillMaxWidth().aspectRatio(0.75f),
                    )
                }
                Text(
                    if (game.favorite) "★ ${game.name}" else game.name,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(game.systemSlug, style = MaterialTheme.typography.labelSmall)
            }
        }
    }

    @Composable
    private fun DetailScreen(game: GameInfo) {
        BackHandler { vm.closeGame() }
        val playFocus = remember { FocusRequester() }
        LaunchedEffect(game.id) { playFocus.requestFocus() }
        LaunchedEffect(Unit) { vm.refreshCollectionsQuietly() }
        Column(
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                val art = vm.boxartPath(game.id)
                if (art != null) {
                    AsyncImage(
                        model = File(art),
                        contentDescription = game.name,
                        modifier = Modifier.weight(0.4f).aspectRatio(0.75f),
                    )
                }
                Column(
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.weight(0.6f),
                ) {
                    Text(game.name, style = MaterialTheme.typography.headlineMedium)
                    Text(game.systemSlug, style = MaterialTheme.typography.titleMedium)
                    game.relPath?.let {
                        Text(it, style = MaterialTheme.typography.bodySmall)
                    }
                    Spacer(Modifier.height(8.dp))
                    Button(
                        onClick = { launchGame(game) },
                        modifier = Modifier.focusRequester(playFocus),
                    ) {
                        Text("▶ Play")
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { vm.toggleFavorite(game) }) {
                            Text(if (game.favorite) "★ Unfavorite" else "☆ Favorite")
                        }
                        OutlinedButton(onClick = { vm.closeGame() }) {
                            Text("Back")
                        }
                    }
                    val manualCollections = vm.collections.filter { it.kind == "manual" }
                    if (manualCollections.isNotEmpty()) {
                        Text("Add to collection", style = MaterialTheme.typography.labelSmall)
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            items(manualCollections) { c ->
                                OutlinedButton(onClick = { vm.addGameToCollection(c.id, game) }) {
                                    Text(c.name)
                                }
                            }
                        }
                    }
                }
            }
            vm.status?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        }
    }

    @Composable
    private fun ScanStatus() {
        vm.progress?.let { (done, total) ->
            if (total > 0uL) {
                LinearProgressIndicator(
                    progress = { done.toFloat() / total.toFloat() },
                    modifier = Modifier.fillMaxWidth(),
                )
            } else {
                CircularProgressIndicator()
            }
        }
        vm.status?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
    }

    private fun launchGame(game: GameInfo) {
        val relPath = game.relPath
        if (relPath == null) {
            vm.status = "No file recorded for this game"
            return
        }
        val rom = File(vm.libraryPath, relPath).absolutePath
        when (
            val result =
                IntentLauncher.launch(this, game.systemSlug, rom, relPath, vm.defaultCore(game.systemSlug))
        ) {
            is IntentLauncher.LaunchResult.Success ->
                vm.recordLaunchStarted(game.id, result.packageName, result.romUri)
            is IntentLauncher.LaunchResult.Failure -> vm.status = result.message
        }
    }
}
