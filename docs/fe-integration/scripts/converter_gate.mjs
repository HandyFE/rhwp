// rhwp converter 회귀 게이트: 운영 서식 전체 × BMS 액션 시나리오를 `rhwp worker` 로 실행해
// 저장 → 재열기 검증(verify) 결과와 PARA_TEXT 컨트롤 순서 비교(ctrlorder.py)를 표로 남긴다.
//
// 사용: node converter_gate.mjs <rhwp(.exe)> <입력 폴더> <출력 폴더> [--image sign.png] [--scenarios file.json]
//       [--only 서식이름부분] [--keep]
//
// 시나리오는 서식의 필드 목록에서 이름 패턴으로 자동 구성한다(§DEFAULT_SCENARIOS). 서식마다 다른
// 셀 이름을 쓰면 --scenarios 로 JSON 을 준다: [{ "name": "...", "actions": [ {CellAction…} ] }]
// 액션의 fieldName 은 "@sign" (첫 서명 셀), "@sign2" (둘째), "@stamp", "@docnum", "@body", "@date"
// 같은 자리표시자를 쓸 수 있다. 그림 액션은 --image 가 있을 때만 실행한다.
//
// 판정: verify.ok 가 false 이거나 워커가 실패하면 FAIL. ctrlorder.py 차이(diff>0)는 WARN 으로 표시한다
// (그림 액션은 표 소유 문단에 그림 컨트롤이 하나 늘어나므로 --ignore-gso 로 비교한다).
// 산출물은 사람이 한글에서 열어 확인한다. 이 스크립트의 PASS 가 한글 표시를 보증하지는 않는다.
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { readdirSync, mkdirSync, writeFileSync, existsSync, rmSync, readFileSync } from 'node:fs';
import { join, resolve, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const argv = process.argv.slice(2);
if (argv.length < 3) {
  console.error('사용: node converter_gate.mjs <rhwp> <입력 폴더> <출력 폴더> [--image sign.png] [--scenarios f.json] [--only 이름] [--keep]');
  process.exit(2);
}
const [rhwpBin, inDir, outDir] = argv.map((a) => resolve(a));
const opt = (k) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : undefined; };
const image = opt('--image') ? resolve(opt('--image')) : undefined;
const scenarioFile = opt('--scenarios');
const only = opt('--only');
const keep = argv.includes('--keep');
const here = dirname(fileURLToPath(import.meta.url));
const ctrlorder = join(here, 'ctrlorder.py');
mkdirSync(outDir, { recursive: true });

const date = `${new Date().getFullYear()}/${String(new Date().getMonth() + 1).padStart(2, '0')}/${String(new Date().getDate()).padStart(2, '0')}`;
const SIGN = { fontName: '굴림', fontSize: '10', fontColor: '0x00000000', fontBold: '1' };
const DATE = { fontName: '굴림', fontSize: '7', fontColor: '0x00000000', fontBold: '1', appendLine: '1' };

// BMS HwpSignActionUtil.makeApprovalSignAction / CallRemoveStampC / OpenAPIProcDocC 가 만드는 조합
const DEFAULT_SCENARIOS = [
  { name: 'sign_text_bottom', actions: [
    { action: 'insertSignTextEx', fieldName: '@sign', inputValue: '홍길동', ...SIGN },
    { action: 'appendSignDateTextEx', fieldName: '@sign', inputValue: date, ...DATE } ] },
  { name: 'sign_text_top', actions: [
    { action: 'insertSignDateTextEx', fieldName: '@sign', inputValue: date, ...SIGN, fontSize: '7' },
    { action: 'appendSignTextEx', fieldName: '@sign', inputValue: '홍길동', ...DATE, fontSize: '10' } ] },
  { name: 'sign_daegyeol', actions: [
    { action: 'insertSignTextEx', fieldName: '@sign', inputValue: '홍길동', ...SIGN },
    { action: 'appendSignDateTextEx', fieldName: '@sign', inputValue: `대결 ${date}`, ...DATE } ] },
  { name: 'sign_two_cells', actions: [
    { action: 'insertSignTextEx', fieldName: '@sign', inputValue: '홍길동', ...SIGN },
    { action: 'appendSignDateTextEx', fieldName: '@sign', inputValue: date, ...DATE },
    { action: 'insertSignTextEx', fieldName: '@sign2', inputValue: '김철수', ...SIGN },
    { action: 'appendSignDateTextEx', fieldName: '@sign2', inputValue: date, ...DATE } ] },
  { name: 'sign_cr_value', actions: [
    { action: 'insertSignTextEx', fieldName: '@sign', inputValue: `10/29\r홍길동`, ...SIGN } ] },
  { name: 'resign_over_existing', actions: [
    { action: 'insertSignTextEx', fieldName: '@sign', inputValue: '첫서명', ...SIGN },
    { action: 'appendSignDateTextEx', fieldName: '@sign', inputValue: date, ...DATE },
    { action: 'insertSignTextEx', fieldName: '@sign', inputValue: '재서명', ...SIGN } ] },
  { name: 'sign_image_bottom', image: true, actions: [
    { action: 'insertImage', fieldName: '@sign', inputValue: '' },
    { action: 'appendSignDateTextEx', fieldName: '@sign', inputValue: date, ...DATE } ] },
  { name: 'sign_image_top', image: true, actions: [
    { action: 'insertSignDateTextEx', fieldName: '@sign', inputValue: date, ...SIGN, fontSize: '7' },
    { action: 'appendPicture', fieldName: '@sign', inputValue: '' } ] },
  { name: 'stamp_remove', actions: [
    { action: 'insertTextEx', fieldName: '@stamp', inputValue: '', ...SIGN } ] },
  { name: 'docnum_clear', actions: [
    { action: 'insertTextEx', fieldName: '@docnum', inputValue: '', ...SIGN } ] },
  { name: 'finish_date', actions: [
    { action: 'insertText', fieldName: '@date', inputValue: date } ] },
  { name: 'body_fill', actions: [
    { action: 'insertText', fieldName: '@body', inputValue: '본문값테스트' } ] },
];
const scenarios = scenarioFile ? JSON.parse(readFileSync(resolve(scenarioFile), 'utf8')) : DEFAULT_SCENARIOS;

// ---------------------------------------------------------------- 워커
class Worker {
  constructor(bin) {
    this.p = spawn(bin, ['worker'], { stdio: ['pipe', 'pipe', 'inherit'] });
    this.rl = createInterface({ input: this.p.stdout });
    this.queue = [];
    this.rl.on('line', (l) => { const r = this.queue.shift(); if (r) r(l); });
  }
  call(req) {
    return new Promise((res, rej) => {
      this.queue.push((l) => { try { res(JSON.parse(l)); } catch (e) { rej(e); } });
      this.p.stdin.write(JSON.stringify(req) + '\n');
    });
  }
  close() { this.p.stdin.end(); }
}

// ---------------------------------------------------------------- 자리표시자
function pick(fields, pattern, nth = 0) {
  const list = fields.filter((f) => pattern.test(f.name));
  return list[nth]?.name;
}
function resolvePlaceholders(fields, actions) {
  const cells = fields.filter((f) => f.cellField);
  const map = {
    '@sign': pick(cells, /^서명\.\d+$|^서명$/, 0) ?? pick(cells, /서명/, 0),
    '@sign2': pick(cells, /^서명\.\d+$/, 1),
    '@stamp': pick(cells, /^관인$|^직인$|^도장$/, 0) ?? pick(cells, /^관인|^직인|^도장/, 0),
    '@docnum': pick(cells, /문서번호/, 0),
    '@date': pick(cells, /시행일|결재일|완료일|날짜|일자/, 0),
    '@body': pick(fields.filter((f) => !f.cellField), /^본문$/, 0) ?? pick(fields, /^본문$/, 0),
  };
  const out = [];
  for (const a of actions) {
    const name = a.fieldName.startsWith('@') ? map[a.fieldName] : a.fieldName;
    if (!name) return { skip: `${a.fieldName} 에 맞는 필드 없음` };
    out.push({ ...a, fieldName: name });
  }
  return { actions: out };
}

// ---------------------------------------------------------------- 실행
const worker = new Worker(rhwpBin);
const pong = await worker.call({ ping: true });
console.error(`rhwp ${pong.version} worker api ${pong.apiVersion}`);
const rows = [];
const files = readdirSync(inDir).filter((x) => /\.hwpx?$/i.test(x) && (!only || x.includes(only)));
for (const f of files) {
  const input = join(inDir, f);
  const fl = await worker.call({ op: 'fields', input, option: 2 });
  if (!fl.ok) { rows.push({ file: f, scenario: '(fields)', result: 'FAIL', error: fl.error }); continue; }
  for (const sc of scenarios) {
    const row = { file: f, scenario: sc.name };
    if (sc.image && !image) { row.result = 'SKIP'; row.note = '--image 없음'; rows.push(row); continue; }
    const r = resolvePlaceholders(fl.fields, sc.actions);
    if (r.skip) { row.result = 'SKIP'; row.note = r.skip; rows.push(row); continue; }
    const actions = r.actions.map((a) => (a.action === 'insertImage' || a.action === 'appendPicture') ? { ...a, imagePath: image } : a);
    const output = join(outDir, `${basename(f).replace(/\.(hwpx?)$/i, '')}__${sc.name}.${f.split('.').pop()}`);
    if (existsSync(output)) rmSync(output);
    const resp = await worker.call({ op: 'convert', input, output, actions, verify: true, strictVerify: false });
    row.elapsedMs = resp.elapsedMs;
    row.fields = actions.map((a) => a.fieldName).filter((v, i, s) => s.indexOf(v) === i).join(',');
    if (!resp.ok) { row.result = 'FAIL'; row.error = resp.error; rows.push(row); continue; }
    const v = resp.verify ?? {};
    row.pages = `${v.pageCountBefore}->${v.pageCount}`;
    row.verifyOk = v.ok;
    row.mismatches = (v.mismatches ?? []).join(' | ');
    // PARA_TEXT 컨트롤 순서 비교 (파이썬이 있을 때)
    const hasImage = actions.some((a) => a.action === 'insertImage' || a.action === 'appendPicture');
    const py = spawnSync('python', ['-I', ctrlorder, input, output, ...(hasImage ? ['--ignore-gso'] : [])], { encoding: 'utf8' });
    if (py.status === 0 || py.status === 1) {
      const m = (py.stdout + py.stderr).match(/diff\D*(\d+)/i);
      row.ctrlorderDiff = m ? Number(m[1]) : (py.status === 0 ? 0 : '?');
    } else {
      row.ctrlorderDiff = 'n/a';
    }
    row.result = v.ok ? (row.ctrlorderDiff === 0 || row.ctrlorderDiff === 'n/a' ? 'PASS' : 'WARN') : 'FAIL';
    row.out = output;
    if (!keep && row.result === 'PASS' && process.env.GATE_KEEP_PASS !== '1') {
      // 통과 산출물도 한글 확인용으로 남긴다 — 지우지 않는다
    }
    rows.push(row);
  }
}
worker.close();

const summary = { PASS: 0, WARN: 0, FAIL: 0, SKIP: 0 };
for (const r of rows) summary[r.result] = (summary[r.result] ?? 0) + 1;
const header = ['file', 'scenario', 'result', 'pages', 'elapsedMs', 'ctrlorderDiff', 'fields', 'mismatches', 'error', 'note'];
const tsv = [header.join('\t'), ...rows.map((r) => header.map((h) => String(r[h] ?? '')).join('\t'))].join('\n');
writeFileSync(join(outDir, 'converter_gate.tsv'), tsv + '\n');
writeFileSync(join(outDir, 'converter_gate.json'), JSON.stringify({ rhwp: pong, summary, rows }, null, 2));
console.log(tsv);
console.log(JSON.stringify(summary));
process.exit(summary.FAIL > 0 ? 1 : 0);
