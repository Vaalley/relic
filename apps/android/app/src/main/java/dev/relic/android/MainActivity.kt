package dev.relic.android

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
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
                        if (!vm.hasLibrary) SetupScreen() else LibraryScreen()
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        hasAccess = Environment.isExternalStorageManager()
        if (vm.hasLibrary) vm.refresh()
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
                items(vm.systems) { sys ->
                    FilterChip(
                        selected = vm.selectedSystem == sys.slug,
                        onClick = { vm.selectSystem(sys.slug) },
                        label = { Text("${sys.name} (${sys.gameCount})") },
                    )
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { vm.scanLibrary() }, enabled = !vm.scanning) {
                    Text(if (vm.scanning) "Scanning…" else "Rescan")
                }
                OutlinedButton(onClick = { vm.editLibrary() }, enabled = !vm.scanning) {
                    Text("Change folder")
                }
            }
            ScanStatus()
            LazyVerticalGrid(
                columns = GridCells.Adaptive(120.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(vm.games, key = { it.id }) { game -> GameTile(game.id, game.name, game.systemSlug, game.relPath) }
            }
        }
    }

    @Composable
    private fun GameTile(id: Long, name: String, systemSlug: String, relPath: String?) {
        Card(onClick = { launchGame(systemSlug, relPath) }) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                val art = vm.boxartPath(id)
                if (art != null) {
                    AsyncImage(
                        model = File(art),
                        contentDescription = name,
                        modifier = Modifier.fillMaxWidth().aspectRatio(0.75f),
                    )
                }
                Text(
                    name,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(systemSlug, style = MaterialTheme.typography.labelSmall)
            }
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
