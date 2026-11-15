#include <windows.h>
#include <stdio.h>
#include <string.h>

/*
 * GetStartupInfoW gets a 104-byte destination that starts 16 bytes before the
 * end of a committed page and continues into a decommitted page. The engine
 * must refuse the call (access violation) before writing anything. Returning
 * normally, or printing the marker, means the bad pointer was accepted.
 */
int main(void)
{
    unsigned char *base = VirtualAlloc(NULL, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!base) return 1;
    unsigned char *target = base + 4096 - 16;
    memset(target, 0xAA, 16);
    if (!VirtualFree(base + 4096, 4096, MEM_DECOMMIT)) return 2;

    GetStartupInfoW((LPSTARTUPINFOW)target);

    puts("BADPTR_RETURNED");
    return 0;
}
