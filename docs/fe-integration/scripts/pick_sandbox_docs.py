# BMS hoffice/sancbox 트리에서 rhwp converter 시험 자료 후보를 고른다.
# 규칙: 문서 ID 가 ...002 로 끝나는 파일은 hox(XML). hox > docInfo > formInfo 로 서식을 구분한다.
#       같은 폴더의 ...001(.hwp) 가 본문 HWP 다. wordType 이 hwpweb(웹한글)·hwp2002(데스크톱 한글) 인 것이 HWP 본문이다.
# 사용: python -X utf8 pick_sandbox_docs.py <sancbox 폴더> [--copy <출력 폴더>] [--tsv out.tsv]
#       --copy 를 주면 "<serial>__<서식>__<상태>.hwp" 로 복사해 converter_gate.mjs 입력으로 쓸 수 있다.
#       wordType hwp2002(데스크톱 한글) 본문은 HC20 압축 포장이라 그대로는 못 연다 — --copy 는 그 파일을 "<이름>.hc20" 으로
#       복사하고, 같은 폴더의 Hc20Decode.java(BMS hsfrmwk-3.0.jar 필요)로 벗긴다. 실측: HC20 안은 HWPX 가 대부분이다.
# hox 상태값(FeWebClient src/approvalBox 참조): approvalStatus = apprstatus_ing/finish/reject/…,
#       participant approvalStatus = partapprstatus_draft/will/now/done/reject/cancel/postpone/skip/…, signStyle = text|image,
#       mappingCell/cellName = 그 결재자의 서명 셀 이름(서명.N).
import os, re, sys, shutil, struct

def tag(t, name):
    m = re.search(rf'<{name}(?:\s[^>]*)?>(.*?)</{name}>', t, re.S)
    if not m:
        return ''
    v = re.sub(r'<!\[CDATA\[(.*?)\]\]>', r'\1', m.group(1), flags=re.S)
    return re.sub(r'\s+', ' ', re.sub(r'<[^>]+>', ' ', v)).strip()

def body_kind(path):
    try:
        with open(path, 'rb') as f:
            h = f.read(8)
    except OSError:
        return ''
    if h == b'\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1':
        return 'hwp'
    if h[:2] == b'PK':
        return 'hwpx'
    if h[:4] == b'HC20':
        return 'hc20'
    return ''

def hwp_pages_hint(path):
    # 페이지 수는 열어야 안다. 여기서는 크기만 힌트로 준다.
    return os.path.getsize(path)

def scan(root):
    rows = []
    for d, _, files in os.walk(root):
        for f in files:
            if not re.search(r'002$', f):
                continue
            p = os.path.join(d, f)
            try:
                t = open(p, 'rb').read().decode('utf-8', 'replace')
            except OSError:
                continue
            if '<formInfo' not in t:
                continue
            fi = re.search(r'<formInfo>.*?</formInfo>', t, re.S)
            fi = fi.group(0) if fi else ''
            serial = f[:-3]
            hwp = None
            for cand in (serial + '001', serial + '001.hwp'):
                c = os.path.join(d, cand)
                if os.path.isfile(c):
                    hwp = c
                    break
            parts = re.findall(r'<participant.*?</participant>', t, re.S)
            pstat = {}
            for pt in parts:
                k = tag(pt, 'approvalStatus').replace('partapprstatus_', '')
                pstat[k] = pstat.get(k, 0) + 1
            cells = [tag(pt, 'cellName') for pt in parts]
            rows.append(dict(
                dir=os.path.relpath(d, root), serial=serial,
                draftDate=tag(t, 'draftDate')[:10], approvalDate=tag(t, 'approvalDate')[:10],
                approvalType=tag(t, 'approvalType').replace('apprtype_', ''),
                pageCnt=tag(t, 'pageCnt'), bodySize=tag(t, 'bodySize'),
                participants=len(parts), partStatus=','.join(f'{k}:{v}' for k, v in sorted(pstat.items())),
                signImages=sum(1 for pt in parts if 'signStyle="image"' in pt),
                cells=','.join(c for c in cells if c),
                formName=tag(fi, 'formName'), wordType=tag(fi, 'wordType'), formType=tag(fi, 'formType'),
                formVersion=tag(fi, 'formVersion'), title=tag(t, 'title')[:60],
                apprStatus=tag(t, 'approvalStatus'), docNumber=tag(t, 'displayDocNumber'),
                hwp=hwp, bodyKind=(body_kind(hwp) if hwp else ''), hwpOk=bool(hwp and body_kind(hwp)), hwpBytes=(os.path.getsize(hwp) if hwp else 0)))
    return rows

def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__ or 'usage: pick_sandbox_docs.py <sancbox> [--copy dir] [--tsv file]'); sys.exit(2)
    root = args[0]; copy = None; tsv = None
    if '--copy' in args: copy = args[args.index('--copy') + 1]
    if '--tsv' in args: tsv = args[args.index('--tsv') + 1]
    rows = scan(root)
    cols = ['dir', 'serial', 'formName', 'wordType', 'formType', 'formVersion', 'apprStatus', 'approvalType', 'draftDate', 'approvalDate', 'pageCnt', 'bodySize', 'participants', 'partStatus', 'signImages', 'cells', 'title', 'docNumber', 'bodyKind', 'hwpOk', 'hwpBytes']
    lines = ['\t'.join(cols)] + ['\t'.join(str(r[c]) for c in cols) for r in rows]
    print('\n'.join(lines))
    if tsv:
        open(tsv, 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    picked = [r for r in rows if r['hwpOk'] and r['wordType'] in ('hwpweb', 'hwp2002', 'hwp', '')]
    kinds = {}
    for r in picked:
        kinds[r['bodyKind']] = kinds.get(r['bodyKind'], 0) + 1
    print(f'\n# hox {len(rows)}건, 본문 파일 있는 것 {sum(1 for r in rows if r["hwp"])}건, 후보 {len(picked)}건 {kinds}', file=sys.stderr)
    by_form = {}
    for r in picked:
        by_form.setdefault(r['formName'] or '(서식 정보 없음)', []).append(r)
    for k, v in sorted(by_form.items()):
        st = {}
        for r in v: st[r['apprStatus']] = st.get(r['apprStatus'], 0) + 1
        print(f'#   {k}: {len(v)}건 {st}', file=sys.stderr)
    if copy:
        os.makedirs(copy, exist_ok=True)
        for r in picked:
            ext = {'hwp': '.hwp', 'hwpx': '.hwpx', 'hc20': '.hc20'}[r['bodyKind']]
            name = re.sub(r'[^\w가-힣.-]+', '_', f"{r['serial']}__{r['formName'] or 'noform'}__{(r['apprStatus'] or 'nostatus').replace('apprstatus_', '')}") + ext
            shutil.copy2(r['hwp'], os.path.join(copy, name))
        print(f'# {len(picked)}건을 {copy} 로 복사', file=sys.stderr)

if __name__ == '__main__':
    main()
