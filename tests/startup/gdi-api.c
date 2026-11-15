#include <windows.h>
#include <stdio.h>
#include <string.h>

/*
 * GDI handle table and attribute blocks through the API.
 *
 * Covers: the stock objects of the table's first page; more handles than one 4 KB page of
 * the table holds (24 bytes each, about 170 per page), created and deleted; device contexts
 * made and deleted, each with its attribute block; and a child process that starts with a
 * copy of the table and uses a stock object from it. The attribute contents are not read
 * back here: GetBkColor needs NtGdiGetBkColor, which is not implemented, so it is not used.
 *
 * Prints GDI_API_OK on success, GDI_FAIL with the failed check otherwise.
 * Run with the argument "child" it is the child: it checks the stock brush and exits 0.
 *
 * Run by .github/workflows/startup-smoke.yml: compiled with MinGW and -lgdi32, executed
 * through compat/run.mjs (the InkSoC engine, not Wine). The workflow requires exit code 0
 * and the marker GDI_API_OK; the child's marker GDI_CHILD_OK is printed as well.
 */

static int bad(const char *what)
{
    printf("GDI_FAIL %s (last error %lu)\n", what, (unsigned long)GetLastError());
    return 1;
}

/* The child is a copy of this program: it does a few hundred table operations, so a minute is
   far more than it needs. A child still running after that is a failure, not a pass. */
#define CHILD_WAIT_MS 60000

static int run_child(void)
{
    char path[MAX_PATH];
    char cmd[MAX_PATH + 16];
    STARTUPINFOA si;
    PROCESS_INFORMATION pi;
    DWORD wait, code = 1;
    int rc = 0;

    if (!GetModuleFileNameA(NULL, path, sizeof(path))) return bad("module path");
    snprintf(cmd, sizeof(cmd), "\"%s\" child", path);
    memset(&si, 0, sizeof(si));
    si.cb = sizeof(si);
    memset(&pi, 0, sizeof(pi));
    if (!CreateProcessA(NULL, cmd, NULL, NULL, FALSE, 0, NULL, NULL, &si, &pi)) return bad("create child");

    /* every path below reaches the two CloseHandle calls, once the handles exist */
    wait = WaitForSingleObject(pi.hProcess, CHILD_WAIT_MS);
    if (wait == WAIT_OBJECT_0) {
        if (!GetExitCodeProcess(pi.hProcess, &code)) {
            rc = bad("child exit code");
        } else if (code != 0) {
            printf("GDI_FAIL child exited with %lu\n", (unsigned long)code);
            rc = 1;
        }
    } else if (wait == WAIT_TIMEOUT) {
        printf("GDI_FAIL child still running after %d ms\n", CHILD_WAIT_MS);
        TerminateProcess(pi.hProcess, 1);
        rc = 1;
    } else {
        rc = bad("wait for child");
    }
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return rc;
}

int main(int argc, char **argv)
{
    if (argc > 1 && strcmp(argv[1], "child") == 0) {
        /* the child's table is a copy: the stock brush must be a valid handle there */
        if (!GetStockObject(BLACK_BRUSH)) return bad("child stock brush");
        puts("GDI_CHILD_OK");
        return 0;
    }

    /* the stock objects of the first page */
    if (!GetStockObject(BLACK_BRUSH)) return bad("stock black brush");
    if (!GetStockObject(WHITE_PEN)) return bad("stock white pen");
    if (!GetStockObject(SYSTEM_FONT)) return bad("stock system font");

    /* more handles than one page of the table holds: each one valid, all of them deleted */
    enum { PENS = 400 };
    static HPEN pens[PENS];
    for (int i = 0; i < PENS; i++) {
        pens[i] = CreatePen(PS_SOLID, 1, RGB(i & 255, 0, 0));
        if (!pens[i]) return bad("create pen beyond one page of handles");
    }
    for (int i = 0; i < PENS; i++) {
        if (!DeleteObject(pens[i])) return bad("delete pen");
    }

    /* attribute blocks: each device context's background is 0xFFFFFF when it is made */
    enum { DCS = 100 };
    static HDC dcs[DCS];
    for (int i = 0; i < DCS; i++) {
        dcs[i] = CreateCompatibleDC(NULL);
        if (!dcs[i]) return bad("create device context");
    }
    for (int i = 0; i < DCS; i++) {
        if (!DeleteDC(dcs[i])) return bad("delete device context");
    }

    /* a child process starts with a copy of the table */
    if (run_child()) return 1;

    puts("GDI_API_OK");
    return 0;
}
