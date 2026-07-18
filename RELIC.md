**Relic** is a local-first, zero-telemetry, open-source retro game frontend launcher designed to organize personal ROM collections and boot standalone emulators entirely offline.

Architecturally, Relic is structured as a decoupled monorepo that splits a heavy, platform-agnostic processing engine from native UI shells. This ensures that the heavy lifting of indexing files and parsing metadata can be reused whether running on an Android handheld, a desktop, or an iOS device.

### Architecture & System Summary

* **The Headless Core Engine (`/core`):** This is the local "backend" of the app, running as an embedded library with a clean C-ABI or multiplatform boundary. It takes a root directory path, spins up a dedicated background thread to crawl the filesystem, and indexes files directly into a local SQLite database cache. It handles the parsing of standard offline metadata formats (like `gamelist.xml`) and streams the structured game lists back to the user interface, eliminating any performance penalties caused by slow runtime storage queries.
* **The Android Frontend (`/apps/android`):** A native launcher built with Kotlin and Jetpack Compose that can act as the device's default Home screen. It captures physical controller hardware inputs directly to manage UI focus and navigation without requiring touch controls. To launch games, it bypasses Android’s modern scoped storage limits by translating internal database paths into secure `content://` URIs, injecting emulator-specific arguments, and firing explicit system `Intent` calls with temporary read permissions.
* **The Desktop Shell (`/apps/desktop`):** A lightweight graphical frame designed for Windows, macOS, and Linux. It hooks directly into the same headless core engine to pull cached game arrays, handles smooth grid rendering, and boots target emulators by spawning simple, direct OS child processes.

### Core Philosophy

Relic rejects modern dark patterns by operating with absolute data privacy. It relies entirely on user-provided, locally-stored media assets, features no remote tracking code or web accounts, and uses a memory-efficient lifecycle that drops its graphical footprint the exact moment an emulator is launched to maximize system resources for gameplay.

To build Relic's headless core engine: a native systems language like Zig
