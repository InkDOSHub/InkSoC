#include <windows.h>
#include <stdio.h>

/*
 * VirtualAlloc and VirtualFree through the API: commits inside a reservation,
 * commits that run past it or lie wholly outside it, commits at addresses that
 * were never reserved, repeated commits and decommits, and release.
 *
 * Prints VIRTUALALLOC_API_OK when every expectation holds. Otherwise it prints
 * the failed check and returns 1, which the workflow treats as a failure.
 */

#define ERROR_INVALID_ADDRESS 487

static int bad(const char *what)
{
    printf("VIRTUALALLOC_FAIL %s (last error %lu)\n", what, (unsigned long)GetLastError());
    return 1;
}

/* A call that must fail with ERROR_INVALID_ADDRESS. */
static int expect_invalid(LPVOID result, const char *what)
{
    if (result) {
        printf("VIRTUALALLOC_FAIL %s succeeded\n", what);
        return 1;
    }
    if (GetLastError() != ERROR_INVALID_ADDRESS) {
        printf("VIRTUALALLOC_FAIL %s: last error %lu, expected 487\n", what, (unsigned long)GetLastError());
        return 1;
    }
    return 0;
}

int main(void)
{
    unsigned char *base = VirtualAlloc((LPVOID)0x5B000000, 0x10000, MEM_RESERVE, PAGE_NOACCESS);
    if (!base) return bad("reserve");

    /* commits inside the reservation, once and then again */
    if (!VirtualAlloc(base, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit inside");
    if (!VirtualAlloc(base, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("repeated commit");

    /* commits outside the reservation fail and leave it as it was */
    if (expect_invalid(VirtualAlloc(base + 0xF000, 0x2000, MEM_COMMIT, PAGE_READWRITE), "commit past the end")) return 1;
    if (expect_invalid(VirtualAlloc(base + 0x10000, 0x1000, MEM_COMMIT, PAGE_READWRITE), "commit wholly after")) return 1;
    if (expect_invalid(VirtualAlloc((LPVOID)0x5C000000, 0x1000, MEM_COMMIT, PAGE_READWRITE), "commit never reserved")) return 1;

    /* a reservation cannot be made again over an existing one */
    if (expect_invalid(VirtualAlloc(base, 0x1000, MEM_RESERVE, PAGE_NOACCESS), "reserve over a reservation")) return 1;

    /* decommit a committed page, then again: both succeed */
    if (!VirtualFree(base, 0x1000, MEM_DECOMMIT)) return bad("decommit");
    if (!VirtualFree(base, 0x1000, MEM_DECOMMIT)) return bad("repeated decommit");

    /* reserve and commit in one call at a fixed address */
    if (!VirtualAlloc((LPVOID)0x5B100000, 0x2000, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE)) return bad("fixed reserve and commit");

    /* release the first reservation */
    if (!VirtualFree(base, 0, MEM_RELEASE)) return bad("release");

    puts("VIRTUALALLOC_API_OK");
    return 0;
}
