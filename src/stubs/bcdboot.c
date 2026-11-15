/* bcdboot.exe for InkSoC: Windows always has it in System32, and programs such as
   Rufus read its version at start (they crash when it is missing). InkSoC has no
   boot configuration to write, so running it only says so. */
#include <stdio.h>

int main(void)
{
    fputs("BCDBoot is not available in InkSoC: there is no boot configuration to change.\n", stderr);
    return 1;
}
