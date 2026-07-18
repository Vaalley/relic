package dev.relic.android

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager

/**
 * Alpha launch path: fire RetroArch's RetroActivityFuture directly, per the
 * template in core/data/intents/retroarch.toml. The general data-driven
 * intent-template engine (docs/android-intents.md) replaces this hardcoding
 * once the alpha proves the flow on-device.
 */
object RetroArchLauncher {

    private val PACKAGES = listOf("com.retroarch.aarch64", "com.retroarch")

    fun installedPackage(context: Context): String? =
        PACKAGES.firstOrNull { pkg ->
            try {
                context.packageManager.getPackageInfo(pkg, 0)
                true
            } catch (_: PackageManager.NameNotFoundException) {
                false
            }
        }

    /** Returns null on success, else a human-readable failure reason. */
    fun launch(context: Context, romAbsolutePath: String, coreName: String?): String? {
        val pkg = installedPackage(context)
            ?: return "RetroArch is not installed (looked for ${PACKAGES.joinToString()})"
        if (coreName == null) return "No default core known for this system"

        val intent = Intent().apply {
            setClassName(pkg, "com.retroarch.browser.retroactivity.RetroActivityFuture")
            action = Intent.ACTION_MAIN
            putExtra("ROM", romAbsolutePath)
            putExtra("LIBRETRO", "/data/data/$pkg/cores/${coreName}_libretro_android.so")
            putExtra("CONFIGFILE", "/storage/emulated/0/Android/data/$pkg/files/retroarch.cfg")
            putExtra("QUITFOCUS", true)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        }
        return try {
            context.startActivity(intent)
            null
        } catch (e: Exception) {
            "Launch failed: ${e.message}"
        }
    }
}
