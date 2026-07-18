# ffi/uniffi

UniFFI-generated Kotlin (Android) and Swift (iOS) bindings over `relic-core`.

Deliberately empty until Phase 1: the binding surface is generated from the
`api::Engine` facade once its Phase-1 shape stabilizes, so we don't churn
generated code while the API is still moving. See PLAN.md §2.1 and §4.1.

Planned contents:
- `relic.udl` (or proc-macro annotations in a thin `relic-ffi` crate)
- Gradle/SwiftPM glue consumed by `apps/android` and `apps/ios`
