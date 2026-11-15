#include <windows.h>
#include <stdio.h>
#include <string.h>

/*
 * Invalid destination for GetStartupInfoA or GetStartupInfoW (104 bytes).
 * Build with exactly one of -DPROBE_A / -DPROBE_W and one of
 * -DCASE_WINDOW_END / -DCASE_OUTSIDE.
 *
 *  CASE_WINDOW_END: the 104 bytes start 16 bytes before the end of the
 *                   committed window and run past it, into address space
 *                   that was never allocated.
 *  CASE_OUTSIDE:    the whole destination lies beyond the window.
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

#ifdef CASE_WINDOW_END
    unsigned char *target = base + WINDOW_SIZE - 16;
    memset(target, 0xAA, 16);
#else
    unsigned char *target = base + WINDOW_SIZE + 0x100; /* CASE_OUTSIDE */
#endif

#ifdef PROBE_A
    GetStartupInfoA((LPSTARTUPINFOA)target);
#else
    GetStartupInfoW((LPSTARTUPINFOW)target);
#endif

    puts("BADPTR_RETURNED");
    return 0;
}
