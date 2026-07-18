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
import uniffi.relic_ffi.GameInfo
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import java.io.File

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
            MaterialTheme(colorScheme = darkColorScheme()) {
                Scaffold { padding ->
                    Column(Modifier.padding(padding).padding(12.dp)) {
                        val detail = vm.selectedGame
                        when {
                            !vm.hasLibrary -> SetupScreen()
                            detail != null -> DetailScreen(detail)
                            else -> LibraryScreen()
                        }
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
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
                        onClick = { launchGame(game.systemSlug, game.relPath) },
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

    private fun launchGame(systemSlug: String, relPath: String?) {
        if (relPath == null) {
            vm.status = "No file recorded for this game"
            return
        }
        val rom = File(vm.libraryPath, relPath).absolutePath
        val error = RetroArchLauncher.launch(this, rom, vm.defaultCore(systemSlug))
        if (error != null) vm.status = error
    }
}
