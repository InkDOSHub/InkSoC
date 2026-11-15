#include <windows.h>
#include <stdio.h>

/*
 * VirtualAlloc and VirtualFree through the API.
 *
 * Covers: commits inside, past and outside a reservation; commits at addresses that
 * were never reserved; repeated commits and decommits; decommits that are invalid
 * (never reserved, past the end, across two reservations, overflowing, zero size);
 * MEM_RELEASE with a size, of a non-base address, and of reservations with and without
 * committed pages; fixed reservations that are aligned and misaligned; overflowing
 * addresses and sizes. Each refused call is followed by a check that committed pages
 * kept their contents, since that is the side effect the probe can observe.
 *
 * Prints VIRTUALALLOC_API_OK when every expectation holds. Otherwise it prints the
 * failed check and returns 1, which the workflow treats as a failure.
 *
 * The commit counter and the exact committed ranges are not visible from a program;
 * they are covered by the host tests (accounting_tests in src/engine/src/mmu.rs).
 */

#define ERR_INVALID_ADDRESS   487
#define ERR_INVALID_PARAMETER 87
#define ERR_NOT_ENOUGH_MEMORY 8

static int bad(const char *what)
{
    printf("VIRTUALALLOC_FAIL %s (last error %lu)\n", what, (unsigned long)GetLastError());
    return 1;
}

/* A call that must fail with the given error code. Pass the call's own result as `ok`. */
static int expect_error(int ok, DWORD code, const char *what)
{
    if (ok) {
        printf("VIRTUALALLOC_FAIL %s succeeded\n", what);
        return 1;
    }
    if (GetLastError() != code) {
        printf("VIRTUALALLOC_FAIL %s: last error %lu, expected %lu\n",
               what, (unsigned long)GetLastError(), (unsigned long)code);
        return 1;
    }
    return 0;
}

/* A committed page must still hold what was written to it. */
static int expect_byte(unsigned char *p, unsigned char value, const char *what)
{
    if (*p != value) {
        printf("VIRTUALALLOC_FAIL %s: contents changed\n", what);
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
    if (!VirtualAlloc(base + 0x2000, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit second page");
    base[0x2000] = 0x5A;

    /* commits outside the reservation fail and leave it as it was */
    if (expect_error(VirtualAlloc(base + 0xF000, 0x2000, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_INVALID_ADDRESS, "commit past the end")) return 1;
    if (expect_error(VirtualAlloc(base + 0x10000, 0x1000, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_INVALID_ADDRESS, "commit wholly after")) return 1;
    if (expect_error(VirtualAlloc((LPVOID)0x5C000000, 0x1000, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_INVALID_ADDRESS, "commit never reserved")) return 1;
    if (expect_error(VirtualAlloc(base + 0x8000, (SIZE_T)-0x100, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_INVALID_ADDRESS, "commit with an overflowing size")) return 1;

    /* a reservation cannot be made again over an existing one */
    if (expect_error(VirtualAlloc(base, 0x1000, MEM_RESERVE, PAGE_NOACCESS) != NULL,
                     ERR_INVALID_ADDRESS, "reserve over a reservation")) return 1;
    if (expect_byte(&base[0x2000], 0x5A, "after refused commits")) return 1;

    /* decommit a committed page, then again: both succeed */
    if (!VirtualFree(base, 0x1000, MEM_DECOMMIT)) return bad("decommit");
    if (!VirtualFree(base, 0x1000, MEM_DECOMMIT)) return bad("repeated decommit");

    /* invalid decommits fail, and the committed page next to them keeps its contents */
    if (expect_error(VirtualFree((LPVOID)0x5C000000, 0x1000, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "decommit never reserved")) return 1;
    if (expect_error(VirtualFree(base + 0xF000, 0x2000, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "decommit past the end")) return 1;
    if (expect_error(VirtualFree(base, (SIZE_T)-1, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "decommit with an overflowing size")) return 1;
    if (expect_error(VirtualFree(base + 0x1000, 0, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "zero-size decommit that is not the base")) return 1;
    if (expect_byte(&base[0x2000], 0x5A, "after refused decommits")) return 1;

    /* two adjacent reservations: a decommit that crosses from one into the other is refused,
       and the committed pages on both sides keep their contents */
    unsigned char *r2 = VirtualAlloc((LPVOID)0x5B200000, 0x10000, MEM_RESERVE, PAGE_NOACCESS);
    unsigned char *r3 = VirtualAlloc((LPVOID)0x5B210000, 0x10000, MEM_RESERVE, PAGE_NOACCESS);
    if (!r2 || !r3) return bad("adjacent reservations");
    if (!VirtualAlloc(r2 + 0xF000, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit end of first reservation");
    if (!VirtualAlloc(r3, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit start of second reservation");
    r2[0xF000] = 0xC3;
    r3[0x0000] = 0x3C;
    if (expect_error(VirtualFree(r2 + 0xF000, 0x2000, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "decommit across two reservations")) return 1;
    if (expect_byte(&r2[0xF000], 0xC3, "first reservation after a refused decommit")) return 1;
    if (expect_byte(&r3[0x0000], 0x3C, "second reservation after a refused decommit")) return 1;

    /* a fixed reservation keeps the address it was asked for */
    unsigned char *fixed = VirtualAlloc((LPVOID)0x5B100000, 0x2000, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (fixed != (unsigned char *)0x5B100000) return bad("aligned fixed reserve and commit");

    /* a misaligned fixed reservation: the base is rounded down to its 64 KB unit and the
       end up to a page, so 0x5B180800 + 0x1000 reserves 0x5B180000..0x5B182000 */
    unsigned char *mis = VirtualAlloc((LPVOID)0x5B180800, 0x1000, MEM_RESERVE, PAGE_NOACCESS);
    if (mis != (unsigned char *)0x5B180000) return bad("misaligned fixed reserve rounds the base down");
    /* that 64 KB unit is now taken, so an aligned reservation at its base collides */
    if (expect_error(VirtualAlloc((LPVOID)0x5B180000, 0x1000, MEM_RESERVE, PAGE_NOACCESS) != NULL,
                     ERR_INVALID_ADDRESS, "aligned reserve over a misaligned one")) return 1;

    /* a misaligned fixed reservation with commit: both pages it spans are committed and writable */
    unsigned char *mc = VirtualAlloc((LPVOID)0x5B1F0800, 0x1000, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (mc != (unsigned char *)0x5B1F0000) return bad("misaligned fixed reserve and commit");
    mc[0x0800] = 0x4D;
    mc[0x1800] = 0x4E;
    if (expect_byte(&mc[0x0800], 0x4D, "first page of misaligned commit")) return 1;
    if (expect_byte(&mc[0x1800], 0x4E, "second page of misaligned commit")) return 1;

    /* near the top of a 64 KB unit: the base rounds down to 0x5B1F0000, which the reservation
       just made already holds, so this collides; a size that crosses a page boundary at the
       top of a unit is accepted and rounds to the unit's base */
    if (expect_error(VirtualAlloc((LPVOID)0x5B1FFFF0, 0x10, MEM_RESERVE, PAGE_NOACCESS) != NULL,
                     ERR_INVALID_ADDRESS, "fixed reserve at the top of a unit that is in use")) return 1;
    unsigned char *top = VirtualAlloc((LPVOID)0x5B3FFFF0, 0x20, MEM_RESERVE, PAGE_NOACCESS);
    if (top != (unsigned char *)0x5B3F0000) return bad("fixed reserve across a page boundary at the top of a unit");

    /* overflowing addresses and sizes are refused with the error the engine reports */
    if (expect_error(VirtualAlloc((LPVOID)0xFFFFFFFFFFFF0000ULL, 0x20000, MEM_RESERVE, PAGE_NOACCESS) != NULL,
                     ERR_INVALID_ADDRESS, "fixed reserve that overflows")) return 1;
    if (expect_error(VirtualAlloc(NULL, (SIZE_T)-1, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_NOT_ENOUGH_MEMORY, "commit of an overflowing size")) return 1;
    if (expect_error(VirtualAlloc(NULL, (SIZE_T)-1, MEM_RESERVE, PAGE_NOACCESS) != NULL,
                     ERR_NOT_ENOUGH_MEMORY, "reserve of an overflowing size")) return 1;
    if (expect_byte(&base[0x2000], 0x5A, "after overflowing requests")) return 1;

    /* MEM_RELEASE takes size 0 and the base address; a refused release leaves the region */
    if (expect_error(VirtualFree(base, 0x1000, MEM_RELEASE),
                     ERR_INVALID_PARAMETER, "release with a size")) return 1;
    if (!VirtualAlloc(base + 0x4000, 0x1000, MEM_COMMIT, PAGE_READWRITE))
        return bad("region still there after a refused release");
    if (expect_error(VirtualFree(base + 0x1000, 0, MEM_RELEASE),
                     ERR_INVALID_ADDRESS, "release of a non-base address")) return 1;

    /* release of a reservation with no committed pages */
    unsigned char *r4 = VirtualAlloc((LPVOID)0x5B300000, 0x10000, MEM_RESERVE, PAGE_NOACCESS);
    if (!r4) return bad("reserve for release without commits");
    if (!VirtualFree(r4, 0, MEM_RELEASE)) return bad("release of a reservation with no committed pages");

    /* release of a reservation that is partly committed */
    unsigned char *r5 = VirtualAlloc((LPVOID)0x5B400000, 0x10000, MEM_RESERVE, PAGE_NOACCESS);
    if (!r5 || !VirtualAlloc(r5 + 0x1000, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("partly committed reservation");
    if (!VirtualFree(r5, 0, MEM_RELEASE)) return bad("release of a partly committed reservation");

    /* MEM_DECOMMIT with size 0 at the base decommits the whole region; the reservation
       stays, and committing again gives zeroed pages */
    unsigned char *r6 = VirtualAlloc((LPVOID)0x5B600000, 0x10000, MEM_RESERVE, PAGE_NOACCESS);
    if (!r6 || !VirtualAlloc(r6, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("reserve for zero-size decommit");
    if (!VirtualAlloc(r6 + 0x2000, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit second page for zero-size decommit");
    r6[0x0000] = 0x77;
    r6[0x2000] = 0x88;
    if (expect_error(VirtualFree(r6 + 0x1000, 0, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "zero-size decommit that is not the base")) return 1;
    if (expect_error(VirtualFree((LPVOID)0x5C100000, 0, MEM_DECOMMIT),
                     ERR_INVALID_ADDRESS, "zero-size decommit of a never-reserved address")) return 1;
    if (expect_byte(&r6[0x0000], 0x77, "after refused zero-size decommits")) return 1;
    if (!VirtualFree(r6, 0, MEM_DECOMMIT)) return bad("zero-size decommit at the base");
    if (!VirtualAlloc(r6, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit after the zero-size decommit");
    if (expect_byte(&r6[0x0000], 0x00, "first page after decommit and commit again")) return 1;
    if (!VirtualAlloc(r6 + 0x2000, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit second page again");
    if (expect_byte(&r6[0x2000], 0x00, "second page after decommit and commit again")) return 1;
    if (!VirtualFree(r6, 0, MEM_RELEASE)) return bad("release after zero-size decommit");

    /* NULL base: the size goes up to whole pages, the base is on a 64 KB unit. A one-byte
       reservation is one page long, so the page after it is outside it; a page plus one byte
       commits two pages, so the second page of it is in and the page after that is out. */
    unsigned char *p1 = VirtualAlloc(NULL, 1, MEM_RESERVE, PAGE_NOACCESS);
    unsigned char *p2 = VirtualAlloc(NULL, 0x1001, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!p1 || !p2) return bad("NULL-base reserve and commit");
    if (((UINT_PTR)p1 & 0xFFFF) || ((UINT_PTR)p2 & 0xFFFF)) return bad("NULL-base reservations are not on a 64 KB unit");
    if (!((UINT_PTR)p2 >= (UINT_PTR)p1 + 0x1000 || (UINT_PTR)p1 >= (UINT_PTR)p2 + 0x2000))
        return bad("NULL-base reservations overlap");
    if (!VirtualAlloc(p1, 0x1000, MEM_COMMIT, PAGE_READWRITE)) return bad("commit the one page of a one-byte reservation");
    p1[0] = 0x61;
    p2[0x1000] = 0x62;
    if (expect_error(VirtualAlloc(p1 + 0x1000, 0x1000, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_INVALID_ADDRESS, "commit the page after a one-byte reservation")) return 1;
    if (expect_error(VirtualAlloc(p2 + 0x2000, 0x1000, MEM_COMMIT, PAGE_READWRITE) != NULL,
                     ERR_INVALID_ADDRESS, "commit the page after a page-plus-one reservation")) return 1;
    if (expect_byte(p1, 0x61, "one-byte reservation after refused commit")) return 1;
    if (expect_byte(&p2[0x1000], 0x62, "second page after refused commit")) return 1;
    if (!VirtualFree(p1, 0, MEM_RELEASE)) return bad("release a one-byte reservation");
    if (!VirtualFree(p2, 0, MEM_RELEASE)) return bad("release a page-plus-one reservation");

    /* release the rest */
    if (!VirtualFree(r2, 0, MEM_RELEASE)) return bad("release first adjacent reservation");
    if (!VirtualFree(r3, 0, MEM_RELEASE)) return bad("release second adjacent reservation");
    if (!VirtualFree(base, 0, MEM_RELEASE)) return bad("release");
    if (!VirtualFree(fixed, 0, MEM_RELEASE)) return bad("release fixed reservation");
    if (!VirtualFree(mis, 0, MEM_RELEASE)) return bad("release misaligned fixed reservation");
    if (!VirtualFree(mc, 0, MEM_RELEASE)) return bad("release misaligned fixed reservation with commit");
    if (!VirtualFree(top, 0, MEM_RELEASE)) return bad("release fixed reservation at the top of a unit");

    puts("VIRTUALALLOC_API_OK");
    return 0;
}
