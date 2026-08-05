//
//  lolt.h
//  Bridging header for the Rust core (see ../../src/ffi.rs).
//
//  Both the app and the widget extension compile against this; the symbols come
//  from liblolt_core.a, built by scripts/build-rust.sh.
//

#ifndef LOLT_H
#define LOLT_H

/// Fetch a tracker snapshot and return it as JSON.
///
/// `league` and `team` are NUL-terminated UTF-8, or NULL for the defaults
/// ("lck" / "t1"). `timeout_secs` of 0 means the default (20s).
///
/// The result is an owned C string the caller must release with
/// lolt_string_free(). It is NULL only if allocation failed.
///
/// Shape: {"ok":true,"snapshot":{...}} or {"ok":false,"error":"..."}
///
/// This call performs blocking network I/O — never run it on the main thread.
char *lolt_snapshot_json(const char *league, const char *team,
                         unsigned long long timeout_secs);

/// Release a string returned by this library. NULL is ignored.
void lolt_string_free(char *ptr);

/// Version of the Rust core. Statically allocated; never free it.
const char *lolt_version(void);

#endif /* LOLT_H */
