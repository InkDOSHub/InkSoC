#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

typedef VOID (WINAPI *GETSTARTUPINFOA_FN)(LPSTARTUPINFOA);
typedef VOID (WINAPI *GETSTARTUPINFOW_FN)(LPSTARTUPINFOW);
typedef LPSTR (WINAPI *GETCOMMANDLINEA_FN)(VOID);
typedef LPWSTR (WINAPI *GETCOMMANDLINEW_FN)(VOID);

static int startup_fields_are_zero_a(const STARTUPINFOA *s)
{
    return s->lpTitle == NULL &&
           s->dwX == 0 && s->dwY == 0 &&
           s->dwXSize == 0 && s->dwYSize == 0 &&
           s->dwXCountChars == 0 && s->dwYCountChars == 0 &&
           s->dwFillAttribute == 0 && s->dwFlags == 0 &&
           s->wShowWindow == 0 && s->cbReserved2 == 0 &&
           s->lpReserved2 == NULL;
}

static int startup_fields_are_zero_w(const STARTUPINFOW *s)
{
    return s->lpTitle == NULL &&
           s->dwX == 0 && s->dwY == 0 &&
           s->dwXSize == 0 && s->dwYSize == 0 &&
           s->dwXCountChars == 0 && s->dwYCountChars == 0 &&
           s->dwFillAttribute == 0 && s->dwFlags == 0 &&
           s->wShowWindow == 0 && s->cbReserved2 == 0 &&
           s->lpReserved2 == NULL;
}

int main(void)
{
    STARTUPINFOW wide;
    STARTUPINFOA ansi;

    ZeroMemory(&wide, sizeof(wide));
    GetStartupInfoW(&wide);
    if (wide.cb != sizeof(wide)) return 1;
    if (!wide.lpDesktop || wcscmp(wide.lpDesktop, L"WinSta0\\Default")) return 2;
    if (wide.hStdInput != GetStdHandle(STD_INPUT_HANDLE)) return 3;
    if (wide.hStdOutput != GetStdHandle(STD_OUTPUT_HANDLE)) return 4;
    if (wide.hStdError != GetStdHandle(STD_ERROR_HANDLE)) return 5;
    if (!startup_fields_are_zero_w(&wide)) return 6;

    ZeroMemory(&ansi, sizeof(ansi));
    GetStartupInfoA(&ansi);
    if (ansi.cb != sizeof(ansi)) return 7;
    if (!ansi.lpDesktop || strcmp(ansi.lpDesktop, "WinSta0\\Default")) return 8;
    if (ansi.hStdInput != GetStdHandle(STD_INPUT_HANDLE)) return 9;
    if (ansi.hStdOutput != GetStdHandle(STD_OUTPUT_HANDLE)) return 10;
    if (ansi.hStdError != GetStdHandle(STD_ERROR_HANDLE)) return 11;
    if (!startup_fields_are_zero_a(&ansi)) return 12;

    if (!GetCommandLineA() || !GetCommandLineA()[0]) return 13;
    if (!GetCommandLineW() || !GetCommandLineW()[0]) return 14;

    /*
     * Resolve the same APIs explicitly from KernelBase. This exercises the
     * module/export lookup path, not merely the Kernel32 import thunks.
     */
    HMODULE kernelbase = LoadLibraryA("kernelbase.dll");
    if (!kernelbase) return 15;

    GETSTARTUPINFOA_FN kb_startup_a =
        (GETSTARTUPINFOA_FN)GetProcAddress(kernelbase, "GetStartupInfoA");
    GETSTARTUPINFOW_FN kb_startup_w =
        (GETSTARTUPINFOW_FN)GetProcAddress(kernelbase, "GetStartupInfoW");
    GETCOMMANDLINEA_FN kb_command_a =
        (GETCOMMANDLINEA_FN)GetProcAddress(kernelbase, "GetCommandLineA");
    GETCOMMANDLINEW_FN kb_command_w =
        (GETCOMMANDLINEW_FN)GetProcAddress(kernelbase, "GetCommandLineW");
    if (!kb_startup_a || !kb_startup_w || !kb_command_a || !kb_command_w) return 16;

    ZeroMemory(&wide, sizeof(wide));
    kb_startup_w(&wide);
    if (wide.cb != sizeof(wide) ||
        !wide.lpDesktop || wcscmp(wide.lpDesktop, L"WinSta0\\Default") ||
        wide.hStdInput != GetStdHandle(STD_INPUT_HANDLE) ||
        wide.hStdOutput != GetStdHandle(STD_OUTPUT_HANDLE) ||
        wide.hStdError != GetStdHandle(STD_ERROR_HANDLE) ||
        !startup_fields_are_zero_w(&wide)) return 17;

    ZeroMemory(&ansi, sizeof(ansi));
    kb_startup_a(&ansi);
    if (ansi.cb != sizeof(ansi) ||
        !ansi.lpDesktop || strcmp(ansi.lpDesktop, "WinSta0\\Default") ||
        ansi.hStdInput != GetStdHandle(STD_INPUT_HANDLE) ||
        ansi.hStdOutput != GetStdHandle(STD_OUTPUT_HANDLE) ||
        ansi.hStdError != GetStdHandle(STD_ERROR_HANDLE) ||
        !startup_fields_are_zero_a(&ansi)) return 18;

    if (!kb_command_a() || !kb_command_a()[0] ||
        strcmp(kb_command_a(), GetCommandLineA())) return 19;
    if (!kb_command_w() || !kb_command_w()[0] ||
        wcscmp(kb_command_w(), GetCommandLineW())) return 20;

    puts("INKSOC_STARTUP_INFO_OK");
    return 0;
}
