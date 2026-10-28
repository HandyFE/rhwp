# 모든 PARA_TEXT 의 컨트롤 토큰 순서를 뽑아 두 HWP 를 비교한다 (글자는 무시).
# 사용: python -I ctrlorder.py <원본.hwp> <저장본.hwp>
import struct, sys, zlib, importlib.util, os

spec = importlib.util.spec_from_file_location('pt', os.path.join(os.path.dirname(__file__), 'paratext_lib.py'))
pt = importlib.util.module_from_spec(spec); spec.loader.exec_module(pt)

CHARCTL = {0, 10, 13, 24, 25, 26, 27, 28, 29, 30, 31}

def sections(path):
    data = open(path, 'rb').read()
    ents, read = pt.cfb_streams(data)
    bt = pt.find(ents, ents[0]['child'], 'BodyText')
    out, i = [], 0
    while True:
        s = pt.find(ents, ents[bt]['child'], f'Section{i}')
        if s < 0: break
        raw = read(ents[s])
        try: buf = zlib.decompress(raw, -15)
        except Exception: buf = raw
        out.append(buf); i += 1
    return out

def seqs(path):
    res = []
    for buf in sections(path):
        for tag, lv, body in pt.records(buf):
            if tag != 67: continue
            ws = struct.unpack('<%dH' % (len(body) // 2), body)
            j, toks = 0, []
            while j < len(ws):
                c = ws[j]
                if c < 32 and c not in CHARCTL:
                    cid = struct.pack('<I', ws[j+1] | (ws[j+2] << 16))[::-1].decode('latin1').strip()
                    toks.append('0x4:END' if c == 4 else f'{c:#x}:{cid}'); j += 8
                else:
                    j += 1
            if toks: res.append((lv, toks))
    return res

a, b = seqs(sys.argv[1]), seqs(sys.argv[2])
if len(sys.argv) > 3 and sys.argv[3] == '--ignore-gso':
    a = [(lv, [t for t in ts if t != '0xb:gso']) for lv, ts in a]
    b = [(lv, [t for t in ts if t != '0xb:gso']) for lv, ts in b]
diffs = [(i, x, y) for i, (x, y) in enumerate(zip(a, b)) if x != y]
print(f'paras_with_ctrls orig={len(a)} saved={len(b)} order_diffs={len(diffs)}')
for i, x, y in diffs[:3]:
    print(f'  #{i} lv={x[0]}\n    orig  {" ".join(x[1])}\n    saved {" ".join(y[1])}')
