#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

typedef VOID (WINAPI *GETSTARTUPINFOA_FN)(LPSTARTUPINFOA);
typedef VOID (WINAPI *GETSTARTUPINFOW_FN)(LPSTARTUPINFOW);
typedef LPSTR (WINAPI *GETCOMMANDLINEA_FN)(VOID);
typedef LPWSTR (WINAPI *GETCOMMANDLINEW_FN)(VOID);

/*
 * Values the engine places in the process parameters (winefb.rs,
 * process_parameters). They are distinct and non-zero so that a field read
 * from the wrong offset, or a copy into the wrong member, is detected.
 */
#define SENTINEL_X              101
#define SENTINEL_Y              102
#define SENTINEL_XSIZE          103
#define SENTINEL_YSIZE          104
#define SENTINEL_XCOUNTCHARS    105
#define SENTINEL_YCOUNTCHARS    106
#define SENTINEL_FILLATTRIBUTE  107
#define SENTINEL_FLAGS          0x40000000UL
#define SENTINEL_SHOWWINDOW     7
#define EXE_NAME_W              L"startup-info.exe"
#define EXE_NAME_A              "startup-info.exe"

static int check_numeric_w(const STARTUPINFOW *s)
{
    return s->cb == sizeof(*s) &&
           s->dwX == SENTINEL_X && s->dwY == SENTINEL_Y &&
           s->dwXSize == SENTINEL_XSIZE && s->dwYSize == SENTINEL_YSIZE &&
           s->dwXCountChars == SENTINEL_XCOUNTCHARS &&
           s->dwYCountChars == SENTINEL_YCOUNTCHARS &&
           s->dwFillAttribute == SENTINEL_FILLATTRIBUTE &&
           s->dwFlags == SENTINEL_FLAGS &&
           s->wShowWindow == SENTINEL_SHOWWINDOW &&
           s->cbReserved2 == 0 && s->lpReserved2 == NULL &&
           s->lpReserved == NULL;
}

static int check_numeric_a(const STARTUPINFOA *s)
{
    return s->cb == sizeof(*s) &&
           s->dwX == SENTINEL_X && s->dwY == SENTINEL_Y &&
           s->dwXSize == SENTINEL_XSIZE && s->dwYSize == SENTINEL_YSIZE &&
           s->dwXCountChars == SENTINEL_XCOUNTCHARS &&
           s->dwYCountChars == SENTINEL_YCOUNTCHARS &&
           s->dwFillAttribute == SENTINEL_FILLATTRIBUTE &&
           s->dwFlags == SENTINEL_FLAGS &&
           s->wShowWindow == SENTINEL_SHOWWINDOW &&
           s->cbReserved2 == 0 && s->lpReserved2 == NULL &&
           s->lpReserved == NULL;
}

/* Desktop and title must be valid, stable strings with the expected text. */
static int check_strings_w(const STARTUPINFOW *s)
{
    return s->lpDesktop && wcscmp(s->lpDesktop, L"WinSta0\\Default") == 0 &&
           s->lpTitle && wcscmp(s->lpTitle, EXE_NAME_W) == 0;
}

static int check_strings_a(const STARTUPINFOA *s)
{
    return s->lpDesktop && strcmp(s->lpDesktop, "WinSta0\\Default") == 0 &&
           s->lpTitle && strcmp(s->lpTitle, EXE_NAME_A) == 0;
}

static int check_handles(const STARTUPINFOW *w)
{
    return w->hStdInput == GetStdHandle(STD_INPUT_HANDLE) &&
           w->hStdOutput == GetStdHandle(STD_OUTPUT_HANDLE) &&
           w->hStdError == GetStdHandle(STD_ERROR_HANDLE);
}

int main(void)
{
    STARTUPINFOW wide;
    STARTUPINFOA ansi;

    /* Kernel32 import path, wide */
    ZeroMemory(&wide, sizeof(wide));
    GetStartupInfoW(&wide);
    if (!check_numeric_w(&wide)) return 1;
    if (!check_strings_w(&wide)) return 2;
    if (!check_handles(&wide)) return 3;

    /* Kernel32 import path, ANSI: same values, text converted */
    ZeroMemory(&ansi, sizeof(ansi));
    GetStartupInfoA(&ansi);
    if (!check_numeric_a(&ansi)) return 4;
    if (!check_strings_a(&ansi)) return 5;
    if (ansi.hStdInput != GetStdHandle(STD_INPUT_HANDLE) ||
        ansi.hStdOutput != GetStdHandle(STD_OUTPUT_HANDLE) ||
        ansi.hStdError != GetStdHandle(STD_ERROR_HANDLE)) return 6;

    /* The ANSI copy is the same string on every call (stable lifetime) */
    STARTUPINFOA again;
    ZeroMemory(&again, sizeof(again));
    GetStartupInfoA(&again);
    if (again.lpDesktop != ansi.lpDesktop || again.lpTitle != ansi.lpTitle) return 7;

    if (!GetCommandLineA() || !GetCommandLineA()[0]) return 8;
    if (!GetCommandLineW() || !GetCommandLineW()[0]) return 9;

    /*
     * Resolve the same APIs explicitly from KernelBase. This exercises the
     * module/export lookup path, not merely the Kernel32 import thunks.
     */
    HMODULE kernelbase = LoadLibraryA("kernelbase.dll");
    if (!kernelbase) return 10;

    GETSTARTUPINFOA_FN kb_startup_a =
        (GETSTARTUPINFOA_FN)GetProcAddress(kernelbase, "GetStartupInfoA");
    GETSTARTUPINFOW_FN kb_startup_w =
        (GETSTARTUPINFOW_FN)GetProcAddress(kernelbase, "GetStartupInfoW");
    GETCOMMANDLINEA_FN kb_command_a =
        (GETCOMMANDLINEA_FN)GetProcAddress(kernelbase, "GetCommandLineA");
    GETCOMMANDLINEW_FN kb_command_w =
        (GETCOMMANDLINEW_FN)GetProcAddress(kernelbase, "GetCommandLineW");
    if (!kb_startup_a || !kb_startup_w || !kb_command_a || !kb_command_w) return 11;

    /* KernelBase path, wide */
    ZeroMemory(&wide, sizeof(wide));
    kb_startup_w(&wide);
    if (!check_numeric_w(&wide)) return 12;
    if (!check_strings_w(&wide)) return 13;
    if (!check_handles(&wide)) return 14;

    /* KernelBase path, ANSI */
    ZeroMemory(&ansi, sizeof(ansi));
    kb_startup_a(&ansi);
    if (!check_numeric_a(&ansi)) return 15;
    if (!check_strings_a(&ansi)) return 16;

    if (!kb_command_a() || !kb_command_a()[0] ||
        strcmp(kb_command_a(), GetCommandLineA())) return 17;
    if (!kb_command_w() || !kb_command_w()[0] ||
        wcscmp(kb_command_w(), GetCommandLineW())) return 18;

    puts("INKSOC_STARTUP_INFO_OK");
    return 0;
}
