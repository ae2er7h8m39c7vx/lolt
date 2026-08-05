/*
 * Link-level smoke test for the C ABI the macOS widget uses.
 *
 * Calls every exported symbol without touching the network, so CI can prove
 * that liblolt_core.a still exports them — and, on macOS, that the linker flag
 * list in macos/project.yml is complete. A rename in src/ffi.rs or a stray
 * strip setting breaks the link here rather than inside Xcode.
 *
 * Built by .github/workflows/ci.yml; see that file for the exact flags.
 */

#include <stdio.h>
#include <string.h>

extern char *lolt_snapshot_json(const char *league, const char *team,
                                unsigned long long timeout_secs);
extern void lolt_string_free(char *ptr);
extern const char *lolt_version(void);

/*
 * Taking the address in a volatile global forces the linker to resolve
 * lolt_snapshot_json at any optimisation level, without ever calling it —
 * calling it would perform network I/O. If the symbol is missing from the
 * archive, the link fails here.
 */
typedef char *(*lolt_snapshot_fn)(const char *, const char *, unsigned long long);
static lolt_snapshot_fn volatile keep_snapshot_symbol = lolt_snapshot_json;

int main(void) {
    if (keep_snapshot_symbol == NULL) {
        fprintf(stderr, "lolt_snapshot_json resolved to NULL\n");
        return 1;
    }

    const char *version = lolt_version();
    if (version == NULL || strlen(version) == 0) {
        fprintf(stderr, "lolt_version() returned nothing\n");
        return 1;
    }
    printf("linked against lolt core %s\n", version);

    /* Freeing NULL must be a no-op. */
    lolt_string_free(NULL);

    printf("ok\n");
    return 0;
}
