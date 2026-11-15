#include <windows.h>
#include <stdio.h>

/*
 * NtAllocateVirtualMemory / NtFreeVirtualMemory through ntdll, the path Wine's own
 * VirtualAlloc takes. Covers: a reservation, a commit inside it, a repeated commit, a commit
 * that runs past the end of the reservation (must fail, and the committed page keeps its
 * contents), and a release. Also: MEM_COMMIT alone at a NULL base and outside any reservation
 * (both must fail, and the range stays free), a MEM_RESERVE that returns RegionSize page
 * rounded, and a NULL-base MEM_RESERVE | MEM_COMMIT that is usable and then released.
 *
 * Not checked here: the State that VirtualQuery reports for these pages (MEM_RESERVE or
 * MEM_COMMIT). The engine reports it per page now (kernel32.rs write_region_info), with host
 * tests in mmu.rs page_run_tests; the probe does not assert it yet.
 *
 * Prints NTALLOC_API_OK when every check holds. Run by .github/workflows/startup-smoke.yml:
 * compiled with MinGW, executed through compat/run.mjs (the InkSoC engine, not Wine); the
 * workflow requires exit code 0 and the marker NTALLOC_API_OK.
 */

typedef LONG (WINAPI *NtAllocFn)(HANDLE, PVOID *, ULONG_PTR, PSIZE_T, ULONG, ULONG);
typedef LONG (WINAPI *NtFreeFn)(HANDLE, PVOID *, PSIZE_T, ULONG);

#define MEM_COMMIT_  0x1000
#define MEM_RESERVE_ 0x2000
#define MEM_RELEASE_ 0x8000

static int bad(const char *what, LONG status)
{
    printf("NTALLOC_FAIL %s (status 0x%lx)\n", what, (unsigned long)status);
    return 1;
}

int main(void)
{
    HMODULE ntdll = GetModuleHandleA("ntdll.dll");
    NtAllocFn nt_alloc = ntdll ? (NtAllocFn)GetProcAddress(ntdll, "NtAllocateVirtualMemory") : NULL;
    NtFreeFn nt_free = ntdll ? (NtFreeFn)GetProcAddress(ntdll, "NtFreeVirtualMemory") : NULL;
    if (!nt_alloc || !nt_free) { puts("NTALLOC_FAIL no ntdll entry points"); return 1; }

    HANDLE self = (HANDLE)(LONG_PTR)-1;
    PVOID base = NULL;
    SIZE_T size = 0x10000;
    LONG st = nt_alloc(self, &base, 0, &size, MEM_RESERVE_, PAGE_NOACCESS);
    if (st != 0 || !base) return bad("reserve", st);

    /* commit a page inside the reservation, then again: both succeed, contents kept */
    unsigned char *p = (unsigned char *)base + 0x1000;
    PVOID cb = p;
    SIZE_T cs = 0x1000;
    st = nt_alloc(self, &cb, 0, &cs, MEM_COMMIT_, PAGE_READWRITE);
    if (st != 0) return bad("commit inside the reservation", st);
    p[0] = 0x5A;
    cb = p;
    cs = 0x1000;
    st = nt_alloc(self, &cb, 0, &cs, MEM_COMMIT_, PAGE_READWRITE);
    if (st != 0) return bad("repeated commit", st);
    if (p[0] != 0x5A) { puts("NTALLOC_FAIL contents changed by a repeated commit"); return 1; }

    /* a commit that runs past the end of the reservation fails; the committed page is untouched */
    PVOID ob = (unsigned char *)base + 0xF000;
    SIZE_T os = 0x2000;
    st = nt_alloc(self, &ob, 0, &os, MEM_COMMIT_, PAGE_READWRITE);
    if (st == 0) { puts("NTALLOC_FAIL commit past the end of the reservation succeeded"); return 1; }
    if (p[0] != 0x5A) { puts("NTALLOC_FAIL contents changed by a refused commit"); return 1; }

    /* MEM_COMMIT alone at a NULL base: refused */
    PVOID nb = NULL;
    SIZE_T ns = 0x1000;
    st = nt_alloc(self, &nb, 0, &ns, MEM_COMMIT_, PAGE_READWRITE);
    if (st == 0) { puts("NTALLOC_FAIL MEM_COMMIT alone at a NULL base succeeded"); return 1; }

    /* MEM_COMMIT alone at a fixed address outside any reservation: refused, and the range stays free */
    PVOID fb = (unsigned char *)base + 0x100000;
    SIZE_T fs = 0x1000;
    st = nt_alloc(self, &fb, 0, &fs, MEM_COMMIT_, PAGE_READWRITE);
    if (st == 0) { puts("NTALLOC_FAIL MEM_COMMIT alone outside a reservation succeeded"); return 1; }

    /* MEM_RESERVE at that range now succeeds: the refused commit made no region. RegionSize comes back page rounded */
    fb = (unsigned char *)base + 0x100000;
    fs = 0x1001;
    st = nt_alloc(self, &fb, 0, &fs, MEM_RESERVE_, PAGE_NOACCESS);
    if (st != 0) return bad("reserve after a refused commit", st);
    if (fs != 0x2000) { printf("NTALLOC_FAIL RegionSize 0x%lx, expected 0x2000\n", (unsigned long)fs); return 1; }

    /* release that auxiliary reservation: MEM_RELEASE takes the base and a size of 0 */
    PVOID fr = fb;
    SIZE_T frs = 0;
    st = nt_free(self, &fr, &frs, MEM_RELEASE_);
    if (st != 0) return bad("release of the auxiliary reservation at fb", st);
    /* the range is free again: reserving it once more works, and is released again */
    fb = (unsigned char *)base + 0x100000;
    fs = 0x1000;
    st = nt_alloc(self, &fb, 0, &fs, MEM_RESERVE_, PAGE_NOACCESS);
    if (st != 0) return bad("reserve at fb after its release", st);
    fr = fb;
    frs = 0;
    st = nt_free(self, &fr, &frs, MEM_RELEASE_);
    if (st != 0) return bad("second release of the auxiliary reservation", st);

    /* a NULL-base MEM_RESERVE | MEM_COMMIT: a new range, committed, usable */
    PVOID ab = NULL;
    SIZE_T as = 0x1000;
    st = nt_alloc(self, &ab, 0, &as, MEM_RESERVE_ | MEM_COMMIT_, PAGE_READWRITE);
    if (st != 0 || !ab) return bad("NULL-base reserve and commit", st);
    ((unsigned char *)ab)[0] = 0x3C;
    if (((unsigned char *)ab)[0] != 0x3C) { puts("NTALLOC_FAIL NULL-base committed page not usable"); return 1; }
    PVOID ar = ab;
    SIZE_T ars = 0;
    st = nt_free(self, &ar, &ars, MEM_RELEASE_);
    if (st != 0) return bad("release of the NULL-base range", st);

    /* release the whole reservation */
    PVOID rb = base;
    SIZE_T rs = 0;
    st = nt_free(self, &rb, &rs, MEM_RELEASE_);
    if (st != 0) return bad("release", st);

    puts("NTALLOC_API_OK");
    return 0;
}
