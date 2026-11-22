#!/usr/bin/env python3
"""Builds src/engine/src/winereg.txt: what Wine's DLLs write to the registry when wineboot
registers them (their WINE_REGISTRY .rgs resources: COM classes, interfaces, type libraries,
MIME types, time zones, NLS tables), as a real Wine prefix has it from the start.

Usage: tools/winereg.py bin/wine > src/engine/src/winereg.txt

Output, one line each: "[key]" (below \\Registry\\Machine) starts a key, then
"name<TAB>type<TAB>data" values in it (type s = REG_SZ, d = REG_DWORD, b = REG_BINARY in hex).
"""
import os, re, struct, sys


def resources(path):
    """The WINE_REGISTRY resources of a PE file, as text."""
    b = open(path, 'rb').read()
    pe = struct.unpack_from('<I', b, 0x3c)[0]
    nsec = struct.unpack_from('<H', b, pe + 6)[0]
    optsz = struct.unpack_from('<H', b, pe + 20)[0]
    opt = pe + 24
    dd = opt + (112 if struct.unpack_from('<H', b, opt)[0] == 0x20b else 96)
    rva = struct.unpack_from('<I', b, dd + 2 * 8)[0]
    if not rva:
        return []
    secs = []
    for i in range(nsec):
        vs, va, rs, rp = struct.unpack_from('<IIII', b, pe + 24 + optsz + i * 40 + 8)
        secs.append((va, max(vs, rs), rp))

    def off(r):
        for va, sz, rp in secs:
            if va <= r < va + sz:
                return r - va + rp

    base = off(rva)

    def entries(o):
        nn, ni = struct.unpack_from('<HH', b, base + o + 12)
        for i in range(nn + ni):
            nid, d = struct.unpack_from('<II', b, base + o + 16 + 8 * i)
            if nid & 0x80000000:
                n = struct.unpack_from('<H', b, base + (nid & 0x7fffffff))[0]
                s = base + (nid & 0x7fffffff) + 2
                nid = b[s:s + 2 * n].decode('utf-16le')
            yield nid, d & 0x7fffffff

    out = []
    for t, d in entries(0):
        if t != 'WINE_REGISTRY':
            continue
        for _, d2 in entries(d):
            for _, d3 in entries(d2):
                drva, dsz = struct.unpack_from('<II', b, base + d3)
                out.append(b[off(drva):off(drva) + dsz].decode('latin-1'))
    return out


TOK = re.compile(r"\s*(?:('(?:[^']|'')*')|([{}=])|([^\s{}=']+))")


def toks(t):
    i, out = 0, []
    while True:
        m = TOK.match(t, i)
        if not m or m.end() == i:
            return out
        i = m.end()
        if m.group(1):
            out.append(('s', m.group(1)[1:-1].replace("''", "'")))
        elif m.group(2):
            out.append(('p', m.group(2)))
        else:
            out.append(('w', m.group(3)))


ROOTS = {'HKCR': 'Software\\Classes', 'HKEY_CLASSES_ROOT': 'Software\\Classes', 'HKLM': '', 'HKEY_LOCAL_MACHINE': ''}


def parse(text, module, res):
    """The rgs grammar of Wine's __wine_register_resources (HKCU parts are left out)."""
    tk = toks(text)

    def val(i):
        typ, v = tk[i][1].lower(), tk[i + 1][1].replace('%MODULE%', module)
        if typ == 'd':
            v = str(int(v, 0))
        return i + 2, typ, v

    def body(i, path, out):
        while i < len(tk):
            kind, w = tk[i]
            if (kind, w) == ('p', '}'):
                return i + 1
            if kind == 'w' and w.lower() in ('noremove', 'forceremove', 'delete'):
                i += 1
                continue
            if kind == 'w' and w.lower() == 'val':  # val name = type 'data'
                name = tk[i + 1][1]
                i, typ, v = val(i + 3)
                out.append((path, name, typ, v))
                continue
            sub = (path + '\\' + w).strip('\\')
            i += 1
            out.append((sub, None, None, None))
            if i < len(tk) and tk[i] == ('p', '='):
                i, typ, v = val(i + 1)
                out.append((sub, '', typ, v))
            if i < len(tk) and tk[i] == ('p', '{'):
                i = body(i + 1, sub, out)
        return i

    i = 0
    while i < len(tk):
        root = ROOTS.get(tk[i][1].upper())
        out = []
        i = body(i + 2, root or '', out)
        if root is not None:
            res += out


def main(d):
    res = []
    for f in sorted(os.listdir(d)):
        if f.startswith('wine-') and f.endswith('.dll'):
            for text in resources(os.path.join(d, f)):
                parse(text, 'C:\\Windows\\System32\\' + f[5:], res)
    keys = {}  # key -> {name: (type, data)}, in first-seen order
    for path, name, typ, v in res:
        k = keys.setdefault(path, {})
        if name is not None:
            k.setdefault(name, (typ, v))
    for path, vals in keys.items():
        print('[%s]' % path)
        for name, (typ, v) in vals.items():
            print('%s\t%s\t%s' % (name, typ, v))


if __name__ == '__main__':
    main(sys.argv[1])
