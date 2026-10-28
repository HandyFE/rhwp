// 1단계 시험: 필드명 → 셀 경로 → 셀 크기 맞춤 그림 삽입 → HWP 저장 → 재열기 확인
// 사용: node picture_test.mjs <pkg 디렉터리> <작업 디렉터리>
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [pkgDir, work] = process.argv.slice(2);
const wasm = await import(pathToFileURL(join(pkgDir, 'rhwp.js')).href);
wasm.initSync({ module: readFileSync(join(pkgDir, 'rhwp_bg.wasm')) });

const out = join(work, 'out');
mkdirSync(out, { recursive: true });
const stamp = readFileSync(join(work, 'stamp.png'));
const sign = readFileSync(join(work, 'sign.png'));
const log = [];

function fieldCell(doc, name) {
  const f = JSON.parse(doc.getFieldList()).find((x) => x.name === name);
  if (!f) throw new Error(`필드 없음: ${name}`);
  // WASM getFieldList 형식: location {sectionIndex, paraIndex, path:[{type:'cell', controlIndex, cellIndex, paraIndex}]}
  const n = f.location?.path?.[0];
  if (!n || n.type !== 'cell') return { f, cell: null };
  return { f, cell: { section: f.location.sectionIndex, para: f.location.paraIndex, control: n.controlIndex, cell: n.cellIndex, cellPara: n.paraIndex } };
}

function insertFit(doc, name, img, pxW, pxH, fit) {
  const { cell } = fieldCell(doc, name);
  if (!cell) throw new Error(`${name}: 셀 필드 위치가 아님`);
  const props = JSON.parse(doc.getCellProperties(cell.section, cell.para, cell.control, cell.cell));
  // 셀 안쪽 크기에 비율 유지로 맞춤 (fit 비율 적용)
  const innerW = props.width - (props.paddingLeft ?? 0) - (props.paddingRight ?? 0);
  const innerH = props.height - (props.paddingTop ?? 0) - (props.paddingBottom ?? 0);
  const scale = Math.min(innerW / pxW, innerH / pxH) * fit;
  const w = Math.round(pxW * scale), h = Math.round(pxH * scale);
  const cellPath = JSON.stringify([{ controlIndex: cell.control, cellIndex: cell.cell, cellParaIndex: cell.cellPara }]);
  const r = doc.insertPictureEx(JSON.stringify({
    sectionIdx: cell.section, paraIdx: cell.para, charOffset: 0, cellPath,
    width: w, height: h, naturalWidthPx: pxW, naturalHeightPx: pxH, extension: 'png', description: name,
  }), img);
  log.push({ name, cell, cellSize: { w: props.width, h: props.height }, picture: { w, h }, result: JSON.parse(r) });
}

function run(input, outName, jobs, fills) {
  const doc = new wasm.HwpDocument(readFileSync(join(work, 'in', input)));
  for (const [k, v] of Object.entries(fills ?? {})) doc.setFieldValueByName(k, v);
  for (const j of jobs) {
    try { insertFit(doc, ...j); } catch (e) { log.push({ name: j[0], error: String(e?.message ?? e) }); }
  }
  const bytes = doc.exportHwp();
  writeFileSync(join(out, outName + '.hwp'), bytes);
  // 재열기: 같은 엔진으로 다시 읽어 필드 값·쪽수·SVG 확인
  const re = new wasm.HwpDocument(bytes);
  writeFileSync(join(out, outName + '.svg'), re.renderPageSvg(0));
  log.push({ file: outName, bytes: bytes.length, reopenPages: re.pageCount(),
    reopenValues: Object.fromEntries(Object.keys(fills ?? {}).map((k) => [k, JSON.parse(re.getFieldValueByName(k)).value])) });
}

// 1) 통합서식: 결재란 서명.1·서명.2 칸에 서명 그림, 발신명의 셀에 관인
run('tonghap_blank3.hwp', 'blank3_pictures', [
  ['서명.1', sign, 300, 120, 0.9],
  ['서명.2', sign, 300, 120, 0.9],
  ['발신명의', stamp, 240, 240, 0.9],
], { 발신명의: '핸디소프트 대표이사', '직위.1': '담당', '직위.2': '팀장' });

// 2) FE 관인 표(stamptable2): 셀 필드 '관인'에 관인 그림
run('fe_stamptable2.hwp', 'stamptable2_picture', [['관인', stamp, 240, 240, 1.0]]);

writeFileSync(join(out, 'picture_test.json'), JSON.stringify(log, null, 2));
console.log(JSON.stringify(log, null, 2));
