# Command-line tools (Wine 11.0)

`cmd.exe` and the usual tools of `C:\\Windows\\System32` (attrib, find, findstr, hostname,
ipconfig, reg, sort, taskkill, tasklist, timeout, where, whoami, xcopy): the x86-64 PE builds
from WineHQ's package `wine-stable-amd64 11.0.0.0~noble-1`, unchanged except for stripped debug
symbols (`llvm-strip`). The Command Prompt runs them in the engine like any program.

Wine is free software under the GNU Lesser General Public License 2.1 or later
(https://www.winehq.org/license). Source: https://dl.winehq.org/wine/source/11.0/
