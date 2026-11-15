#include <windows.h>
#include <stdio.h>
#include <string.h>

/*
 * Invalid destination for GetStartupInfoA or GetStartupInfoW (104 bytes).
 * Build with exactly one of -DPROBE_A / -DPROBE_W and one of
 * -DCASE_STRADDLE / -DCASE_UNMAPPED.
 *
 *  CASE_STRADDLE: the first 16 bytes are committed, the rest is not.
 *  CASE_UNMAPPED: the whole destination is outside committed memory.
 *
 * The engine must raise an access violation before writing anything. A normal
 * return prints BADPTR_RETURNED, which the workflow treats as a failure.
 */
#define WINDOW       ((unsigned char *)0x5A000000)
#define WINDOW_SIZE  0x10000

int main(void)
{
    unsigned char *base = VirtualAlloc(WINDOW, WINDOW_SIZE, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!base) return 1; /* inconclusive: the window could not be created */

#ifdef CASE_STRADDLE
    unsigned char *target = base + WINDOW_SIZE - 16;
    memset(target, 0xAA, 16);
#else
    unsigned char *target = base + WINDOW_SIZE + 0x100;
#endif

#ifdef PROBE_A
    GetStartupInfoA((LPSTARTUPINFOA)target);
#else
    GetStartupInfoW((LPSTARTUPINFOW)target);
#endif

    puts("BADPTR_RETURNED");
    return 0;
}
