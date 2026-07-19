package dev.relic.android

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import androidx.core.content.FileProvider
import uniffi.relic_ffi.intentTemplatesForSystem
import uniffi.relic_ffi.resolveIntent
import java.io.File

/**
 * Data-driven Android launch resolver (docs/android-intents.md): builds and
 * fires an explicit [Intent] from the TOML templates under
 * `core/data/intents/`, via relic-core's `intents::resolve`, exposed over
 * UniFFI as `intentTemplatesForSystem`/`resolveIntent`. Replaces the
 * RetroArch-only alpha hardcoding that used to live here as
 * `RetroArchLauncher`.
 *
 * On success, the caller (`RelicViewModel.recordLaunchStarted`) starts a play
 * session and remembers the granted package + URI so `MainActivity.onResume`
 * can record the session's end and revoke the grant when the emulator
 * returns control — there's no session-lifecycle watchdog beyond that
 * resume hook (docs/android-intents.md §5 step 10 relies on it).
 */
object IntentLauncher {

    sealed class LaunchResult {
        data class Success(val packageName: String, val romUri: Uri) : LaunchResult()

        data class Failure(val message: String) : LaunchResult()
    }

    fun launch(
        context: Context,
        systemSlug: String,
        romAbsolutePath: String,
        romRelPath: String,
        defaultCore: String?,
    ): LaunchResult {
        val candidates = intentTemplatesForSystem(systemSlug)
        if (candidates.isEmpty()) {
            return LaunchResult.Failure("No emulator template registered for system '$systemSlug'")
        }

        val template =
            candidates.firstOrNull { isInstalled(context, it.`package`) }
                ?: return LaunchResult.Failure(
                    "None of the candidate emulators are installed (tried " +
                        "${candidates.joinToString { it.displayName }})",
                )

        val romUri =
            FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", File(romAbsolutePath))

        // {core} is libretro-frontend-only (docs/android-intents.md §4.3); both
        // RetroArch package-alias templates start with "retroarch".
        val corePath =
            if (template.id.startsWith("retroarch") && defaultCore != null) {
                "/data/data/${template.`package`}/cores/${defaultCore}_libretro_android.so"
            } else {
                null
            }

        val resolved =
            resolveIntent(template.id, systemSlug, romUri.toString(), romRelPath, corePath)
                ?: return LaunchResult.Failure("Failed to resolve intent template '${template.id}'")

        val intent = Intent(resolved.action)
        intent.setClassName(resolved.`package`, resolved.activity)

        if (resolved.dataMode == "data") {
            val mime = resolved.dataMimeType
            if (mime != null) {
                intent.setDataAndType(romUri, mime)
            } else {
                intent.data = romUri
            }
        }

        for (extra in resolved.extras) {
            when (extra.extraType) {
                "string" -> intent.putExtra(extra.name, extra.value)
                "bool" -> intent.putExtra(extra.name, extra.value == "true")
                "int" -> intent.putExtra(extra.name, extra.value.toInt())
            }
        }

        for (flagName in resolved.flags) {
            flagValue(flagName)?.let { intent.addFlags(it) }
        }

        context.grantUriPermission(resolved.`package`, romUri, Intent.FLAG_GRANT_READ_URI_PERMISSION)

        return try {
            context.startActivity(intent)
            LaunchResult.Success(resolved.`package`, romUri)
        } catch (e: Exception) {
            LaunchResult.Failure("Launch failed: ${e.message}")
        }
    }

    private fun isInstalled(context: Context, packageName: String): Boolean =
        try {
            context.packageManager.getPackageInfo(packageName, 0)
            true
        } catch (_: PackageManager.NameNotFoundException) {
            false
        }

    /** Mirrors relic-core's `KNOWN_FLAGS` (docs/android-intents.md §4.5). */
    private fun flagValue(name: String): Int? =
        when (name) {
            "FLAG_GRANT_READ_URI_PERMISSION" -> Intent.FLAG_GRANT_READ_URI_PERMISSION
            "FLAG_ACTIVITY_NEW_TASK" -> Intent.FLAG_ACTIVITY_NEW_TASK
            "FLAG_ACTIVITY_CLEAR_TOP" -> Intent.FLAG_ACTIVITY_CLEAR_TOP
            "FLAG_ACTIVITY_SINGLE_TOP" -> Intent.FLAG_ACTIVITY_SINGLE_TOP
            "FLAG_ACTIVITY_NO_HISTORY" -> Intent.FLAG_ACTIVITY_NO_HISTORY
            "FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS" -> Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS
            else -> null
        }
}
