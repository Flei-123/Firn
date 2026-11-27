Firn for Windows (x86-64): compiler, formatter, VS Code extension.

1. Unzip.
2. Double-click INSTALL.cmd.
3. Open a new terminal: firn new hello ; cd hello ; firn run
4. VS Code: open the folder, open a .fi file, press F5.

Contents: bin\firnc.exe, bin\firnfmt.exe, bin\firn.cmd, bin\as.exe and ld.exe (GNU binutils for MinGW, used as
x86_64-w64-mingw32-as/ld by firnc), lib\ (standard library), vscode-firn-*.vsix, licenses\.
GNU binutils (GPLv3) comes unmodified from MSYS2: https://packages.msys2.org/package/mingw-w64-x86_64-binutils
