//! Compiles the vendored `rc_hash` subset of `rcheevos`
//! (`native/rcheevos/`, see `native/rcheevos/VENDORED.md`) into a static lib
//! linked into this crate. Disc, encrypted, and rc_hash's own zip handling
//! are compiled out (design doc §2.2 / `VENDORED.md`) — cartridge-console
//! hashing only, for sub-phase 6a.

fn main() {
    let native = "native/rcheevos";

    cc::Build::new()
        .include(format!("{native}/include"))
        .include(native) // rc_compat.h is included as "../rc_compat.h" from src/rhash/*.c
        .file(format!("{native}/src/rc_compat.c"))
        .file(format!("{native}/src/rhash/hash.c"))
        .file(format!("{native}/src/rhash/hash_rom.c"))
        .file(format!("{native}/src/rhash/md5.c"))
        .define("RC_HASH_NO_DISC", None)
        .define("RC_HASH_NO_ENCRYPTED", None)
        .define("RC_HASH_NO_ZIP", None)
        .warnings(false) // vendored upstream source; not ours to lint
        .compile("rc_hash");

    println!("cargo:rerun-if-changed={native}");
}
