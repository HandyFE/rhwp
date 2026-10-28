// 호출 비용 실측: (1) 매 요청 CLI 프로세스 생성 (2) 상주 Node+WASM 반복 처리
// 사용: node bench.mjs <rhwp.exe> <pkg> <입력.hwp> <fill.json> <stamp.png> <N>
import { readFileSync, writeFileSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const [exe, pkgDir, input, fillPath, stampPath, nArg] = process.argv.slice(2);
const N = Number(nArg ?? 20);
const work = mkdtempSync(join(tmpdir(), 'rhwp-bench-'));
const fills = JSON.parse(readFileSync(fillPath, 'utf8'));
const stamp = readFileSync(stampPath);
const src = readFileSync(input);

function stats(label, ms) {
  const s = [...ms].sort((a, b) => a - b);
  const p = (q) => s[Math.min(s.length - 1, Math.floor(q * s.length))];
  console.log(`${label}: n=${s.length} avg=${(s.reduce((a, b) => a + b, 0) / s.length).toFixed(1)}ms p50=${p(0.5).toFixed(1)} p95=${p(0.95).toFixed(1)} max=${s[s.length - 1].toFixed(1)}`);
}

// (1) CLI: 요청마다 프로세스 1개 (fill-fields)
const cli = [];
for (let i = 0; i < N; i++) {
  const t = performance.now();
  execFileSync(exe, ['edit', 'fill-fields', input, '--data', '@' + fillPath, '-o', join(work, `cli_${i}.hwp`), '--json'], { stdio: ['ignore', 'pipe', 'pipe'] });
  cli.push(performance.now() - t);
}
stats('CLI 프로세스/요청 (fill-fields 16필드)', cli);

// (1-b) 프로세스 기동 자체: 가장 가벼운 명령
const boot = [];
for (let i = 0; i < N; i++) {
  const t = performance.now();
  execFileSync(exe, ['--version'], { stdio: ['ignore', 'pipe', 'pipe'] });
  boot.push(performance.now() - t);
}
stats('CLI 기동만 (--version)', boot);

// (2) 상주: WASM 1회 초기화 후 요청마다 열기→채우기→그림→저장
const t0 = performance.now();
const wasm = await import(pathToFileURL(join(pkgDir, 'rhwp.js')).href);
wasm.initSync({ module: readFileSync(join(pkgDir, 'rhwp_bg.wasm')) });
console.log(`WASM 초기화(1회): ${(performance.now() - t0).toFixed(1)}ms`);

const resident = [], open = [], fill = [], pic = [], save = [];
for (let i = 0; i < N; i++) {
  let t = performance.now();
  const start = t;
  const doc = new wasm.HwpDocument(src);
  open.push(performance.now() - t);
  t = performance.now();
  for (const [k, v] of Object.entries(fills)) doc.setFieldValueByName(k, v);
  fill.push(performance.now() - t);
  t = performance.now();
  const f = JSON.parse(doc.getFieldList()).find((x) => x.name === '서명.1');
  const n = f.location.path[0];
  doc.insertPictureEx(JSON.stringify({ sectionIdx: f.location.sectionIndex, paraIdx: f.location.paraIndex, charOffset: 0,
    cellPath: JSON.stringify([{ controlIndex: n.controlIndex, cellIndex: n.cellIndex, cellParaIndex: n.paraIndex }]),
    width: 3000, height: 3000, naturalWidthPx: 240, naturalHeightPx: 240, extension: 'png', description: 'bench' }), stamp);
  pic.push(performance.now() - t);
  t = performance.now();
  const bytes = doc.exportHwp();
  save.push(performance.now() - t);
  doc.free?.();
  resident.push(performance.now() - start);
  if (i === 0) writeFileSync(join(work, 'resident_0.hwp'), bytes);
}
stats('상주 WASM 요청 전체', resident);
stats('  열기', open); stats('  필드 16개 채우기', fill); stats('  그림 1개', pic); stats('  HWP 저장', save);
console.log(`메모리 rss=${(process.memoryUsage().rss / 1048576).toFixed(0)}MB`);
