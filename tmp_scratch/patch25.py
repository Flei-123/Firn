p = '/root/firn-wt-db/compiler/src/win.rs'
s = open(p).read()
old = '    ("FlushFileBuffers", "KERNEL32.dll", 1),\n'
assert old in s
new = old + '''    // lib/db (the embedded database): shorten a file, ask its size, and lock the byte
    // ranges SQLite's own Windows VFS locks (LockFileEx / UnlockFileEx).
    ("SetEndOfFile", "KERNEL32.dll", 1),
    ("GetFileSizeEx", "KERNEL32.dll", 2),
    ("LockFileEx", "KERNEL32.dll", 6),
    ("UnlockFileEx", "KERNEL32.dll", 5),
'''
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print('ok25')
