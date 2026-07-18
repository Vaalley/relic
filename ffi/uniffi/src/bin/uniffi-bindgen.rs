//! Binding generator entry point (library mode): builds Kotlin/Swift sources
//! from the compiled cdylib. See ffi/uniffi/README.md for usage.

fn main() {
    uniffi::uniffi_bindgen_main()
}
