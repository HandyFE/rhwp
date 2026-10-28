// FE 서식 전체: (a) 모든 필드에 글자 채움 (b) 모든 셀 필드에 그림 → 저장 → 재열기 점검
// 사용: node formsweep.mjs <pkg> <in 디렉터리> <out 디렉터리> <stamp.png>
import { readFileSync, writeFileSync, readdirSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [pkgDir, inDir, outDir, stampPath] = process.argv.slice(2);
mkdirSync(outDir, { recursive: true });
const wasm = await import(pathToFileURL(join(pkgDir, 'rhwp.js')).href);
wasm.initSync({ module: readFileSync(join(pkgDir, 'rhwp_bg.wasm')) });
const stamp = readFileSync(stampPath);
const rows = [];

for (const f of readdirSync(inDir).filter((x) => x.endsWith('.hwp'))) {
  const src = readFileSync(join(inDir, f));
  for (const mode of ['text', 'picture']) {
    const row = { file: f, mode };
    try {
      const doc = new wasm.HwpDocument(src);
      const pages0 = doc.pageCount();
      const fields = JSON.parse(doc.getFieldList());
      const names = [...new Set(fields.map((x) => x.name).filter(Boolean))];
      let done = 0, failed = 0;
      if (mode === 'text') {
        for (const n of names) {
          try { const r = JSON.parse(doc.setFieldValueByName(n, `값-${n}`)); r.ok === false ? failed++ : done++; } catch { failed++; }
        }
      } else {
        for (const x of fields.filter((y) => y.location?.path?.[0]?.type === 'cell')) {
          const n = x.location.path[0];
          try {
            doc.insertPictureEx(JSON.stringify({ sectionIdx: x.location.sectionIndex, paraIdx: x.location.paraIndex, charOffset: 0,
              cellPath: JSON.stringify([{ controlIndex: n.controlIndex, cellIndex: n.cellIndex, cellParaIndex: n.paraIndex }]),
              width: 2000, height: 2000, naturalWidthPx: 240, naturalHeightPx: 240, extension: 'png', description: x.name }), stamp);
            done++;
          } catch { failed++; }
        }
      }
      const bytes = doc.exportHwp();
      const out = join(outDir, `${f.replace(/\.hwp$/, '')}__${mode}.hwp`);
      writeFileSync(out, bytes);
      const re = new wasm.HwpDocument(bytes);
      const back = mode === 'text'
        ? names.filter((n) => { try { return JSON.parse(re.getFieldValueByName(n)).value !== `값-${n}`; } catch { return true; } }).length
        : 0;
      Object.assign(row, { fields: names.length, done, failed, pages: `${pages0}->${re.pageCount()}`, valueMismatch: back, out });
    } catch (e) {
      row.error = String(e?.message ?? e);
    }
    rows.push(row);
  }
}
writeFileSync(join(outDir, 'formsweep.json'), JSON.stringify(rows, null, 2));
for (const r of rows) console.log(JSON.stringify(r));
