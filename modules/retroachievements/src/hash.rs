//! FFI binding to the vendored `rc_hash` engine (`native/rcheevos/`, design
//! doc §2.2, sub-phase 6a). `console_id` is `relic_core::systems::System::
//! ra_console_id` — the two are the same enum by construction (Relic's
//! systems registry copies `rc_consoles.h`'s ids verbatim).
//!
//! Only cartridge/ROM consoles hash successfully today: disc, encrypted, and
//! rc_hash's own zip handling are compiled out of the vendored subset (see
//! `native/rcheevos/VENDORED.md`). Archived ROMs already extracted to memory
//! by the core scanner's zip/7z support should go through [`hash_buffer`]
//! rather than [`hash_file`] — that is the intended way archives reach
//! `rc_hash` in this module (no on-disk extraction, no rc_hash zip support
//! needed).

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uint};
use std::path::Path;

// SAFETY: this mirrors `include/rc_hash.h`'s `rc_hash_generate_from_buffer`
// and `rc_hash_generate_from_file` exactly (signature, calling convention).
// Both write a 32-hex-char MD5-shaped string + NUL into `hash` and return
// non-zero on success. Neither retains the pointers after returning.
unsafe extern "C" {
    fn rc_hash_generate_from_buffer(
        hash: *mut c_char,
        console_id: c_uint,
        buffer: *const u8,
        buffer_size: usize,
    ) -> c_int;

    fn rc_hash_generate_from_file(
        hash: *mut c_char,
        console_id: c_uint,
        path: *const c_char,
    ) -> c_int;
}

/// A `rc_hash` output: lowercase 32-char hex, the same shape as an MD5 hex
/// digest (most consoles hash is literally an MD5; the RA-specific rules are
/// in what's fed to MD5 — header stripped, disc region selected, etc. —
/// design doc §2.1). Not comparable to `core::scan::hash`'s generic
/// whole-file MD5 for consoles that strip a header or hash something other
/// than the raw bytes.
pub type RaHash = String;

/// Hash an in-memory buffer for `console_id` (the RA console id, e.g. from
/// `System::ra_console_id`). This is the path for archived ROMs the core
/// scanner has already extracted to memory (design doc §2.2) as well as
/// plain files the caller has already read. Returns `None` if `rc_hash`
/// can't produce a hash for this console/buffer combination (e.g. malformed
/// input, or a console needing disc/zip support this vendored subset
/// excludes).
pub fn hash_buffer(console_id: u32, buffer: &[u8]) -> Option<RaHash> {
    let mut out = [0 as c_char; 33];
    // SAFETY: `out` is 33 bytes (32 hex chars + NUL), matching `char[33]` in
    // the C signature; `buffer`/`buffer.len()` describe a single valid
    // borrowed slice for the duration of this call and are not retained.
    let ok = unsafe {
        rc_hash_generate_from_buffer(out.as_mut_ptr(), console_id, buffer.as_ptr(), buffer.len())
    };
    read_hash(ok, &out)
}

/// Hash a file on disk for `console_id`. Prefer [`hash_buffer`] for archive
/// members already in memory — this opens and reads `path` itself.
pub fn hash_file(console_id: u32, path: &Path) -> Option<RaHash> {
    let path_str = path.to_str()?;
    let c_path = CString::new(path_str).ok()?;
    let mut out = [0 as c_char; 33];
    // SAFETY: `out` is 33 bytes; `c_path` is a valid NUL-terminated buffer
    // kept alive for the duration of this call.
    let ok = unsafe { rc_hash_generate_from_file(out.as_mut_ptr(), console_id, c_path.as_ptr()) };
    read_hash(ok, &out)
}

fn read_hash(ok: c_int, out: &[c_char; 33]) -> Option<RaHash> {
    if ok == 0 {
        return None;
    }
    // SAFETY: on success rc_hash always writes a NUL-terminated string of at
    // most 32 chars into a 33-byte buffer.
    let bytes: Vec<u8> = out
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // RC_CONSOLE_MEGA_DRIVE (core/data/systems/megadrive.toml's ra_console_id):
    // no header stripping, so rc_hash's output must equal a plain MD5 of the
    // whole buffer — cross-checked against an independent MD5 implementation
    // rather than rc_hash itself, per design doc open question #1.
    const RC_CONSOLE_MEGA_DRIVE: u32 = 1;
    // RC_CONSOLE_NINTENDO (core/data/systems/nes.toml): strips a 16-byte
    // "NES\x1a" iNES header before hashing (confirmed against rc_hash's own
    // source, src/rhash/hash_rom.c `rc_hash_nes`).
    const RC_CONSOLE_NINTENDO: u32 = 7;

    fn md5_hex(data: &[u8]) -> String {
        use md5::{Digest, Md5};
        let mut hasher = Md5::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    #[test]
    fn plain_console_hash_matches_whole_buffer_md5() {
        let payload = b"relic fixture bytes, not real ROM content, phase 6a smoke test";
        let got = hash_buffer(RC_CONSOLE_MEGA_DRIVE, payload).expect("rc_hash should succeed");
        assert_eq!(got, md5_hex(payload));
    }

    #[test]
    fn nes_header_is_stripped_before_hashing() {
        let mut with_header = vec![b'N', b'E', b'S', 0x1a];
        with_header.extend_from_slice(&[0u8; 12]); // rest of the 16-byte iNES header
        let payload = b"relic fixture bytes standing in for PRG/CHR data";
        with_header.extend_from_slice(payload);

        let got = hash_buffer(RC_CONSOLE_NINTENDO, &with_header).expect("rc_hash should succeed");
        assert_eq!(
            got,
            md5_hex(payload),
            "NES hash must ignore the 16-byte iNES header, matching rc_hash_nes in hash_rom.c"
        );
    }

    #[test]
    fn unknown_console_id_returns_none_rather_than_garbage() {
        // console id 0 is RC_CONSOLE_UNKNOWN; rc_hash has no rule for it.
        assert!(hash_buffer(0, b"anything").is_none());
    }
}
