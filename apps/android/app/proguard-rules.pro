# JNA + UniFFI bindings are reached reflectively; keep them intact.
-keep class com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-dontwarn java.awt.*
