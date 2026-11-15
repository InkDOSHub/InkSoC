#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

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

    ZeroMemory(&ansi, sizeof(ansi));
    GetStartupInfoA(&ansi);
    if (ansi.cb != sizeof(ansi)) return 6;
    if (!ansi.lpDesktop || strcmp(ansi.lpDesktop, "WinSta0\\Default")) return 7;
    if (ansi.hStdInput != GetStdHandle(STD_INPUT_HANDLE)) return 8;
    if (ansi.hStdOutput != GetStdHandle(STD_OUTPUT_HANDLE)) return 9;
    if (ansi.hStdError != GetStdHandle(STD_ERROR_HANDLE)) return 10;

    if (!GetCommandLineW() || !GetCommandLineW()[0]) return 11;

    puts("INKSOC_STARTUP_INFO_OK");
    return 0;
}
