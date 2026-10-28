# 최소 CFB 리더: BodyText/Section0 의 첫 최상위 문단 PARA_HEADER·PARA_TEXT 순서 출력
import struct, sys, zlib

def cfb_streams(data):
    ssz = 1 << struct.unpack_from('<H', data, 30)[0]
    mssz = 1 << struct.unpack_from('<H', data, 32)[0]
    nfat, dirstart, _, cutoff, mfstart, nmf, difstart, ndif = struct.unpack_from('<IIIIIIII', data, 44)
    difat = list(struct.unpack_from('<109I', data, 76))
    s = difstart
    while s < 0xFFFFFFFA:
        off = 512 + s * ssz
        vals = struct.unpack_from('<%dI' % (ssz // 4), data, off)
        difat += vals[:-1]; s = vals[-1]
    fat = []
    for sec in difat[:nfat]:
        fat += struct.unpack_from('<%dI' % (ssz // 4), data, 512 + sec * ssz)
    def chain(start):
        out = []
        while start < 0xFFFFFFFA:
            out.append(data[512 + start * ssz: 512 + (start + 1) * ssz]); start = fat[start]
        return b''.join(out)
    d = chain(dirstart)
    ents = []
    for i in range(len(d) // 128):
        e = d[i*128:(i+1)*128]
        nl = struct.unpack_from('<H', e, 64)[0]
        name = e[:max(nl-2, 0)].decode('utf-16le')
        typ = e[66]; left, right, child = struct.unpack_from('<iii', e, 68)
        start, size = struct.unpack_from('<II', e, 116)
        ents.append(dict(name=name, typ=typ, left=left, right=right, child=child, start=start, size=size))
    root = ents[0]
    ministream = chain(root['start'])
    mfat = []
    s = mfstart
    while s < 0xFFFFFFFA:
        mfat += struct.unpack_from('<%dI' % (ssz // 4), data, 512 + s * ssz); s = fat[s]
    def read(e):
        if e['size'] < cutoff:
            out, s = [], e['start']
            while s < 0xFFFFFFFA:
                out.append(ministream[s*mssz:(s+1)*mssz]); s = mfat[s]
            return b''.join(out)[:e['size']]
        return chain(e['start'])[:e['size']]
    return ents, read

def find(ents, idx, name):
    stack = [idx]
    while stack:
        i = stack.pop()
        if i < 0: continue
        if ents[i]['name'] == name: return i
        stack += [ents[i]['left'], ents[i]['right']]
    return -1

def records(buf):
    p = 0
    while p + 4 <= len(buf):
        h = struct.unpack_from('<I', buf, p)[0]; p += 4
        tag, lv, sz = h & 0x3FF, (h >> 10) & 0x3FF, h >> 20
        if sz == 0xFFF: sz = struct.unpack_from('<I', buf, p)[0]; p += 4
        yield tag, lv, buf[p:p+sz]; p += sz

