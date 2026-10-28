// 변경 없이 열기 → exportHwp 왕복. 사용: node roundtrip.mjs <pkg> <입력> <출력>
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [pkgDir, input, output] = process.argv.slice(2);
const wasm = await import(pathToFileURL(join(pkgDir, 'rhwp.js')).href);
wasm.initSync({ module: readFileSync(join(pkgDir, 'rhwp_bg.wasm')) });
const doc = new wasm.HwpDocument(readFileSync(input));
writeFileSync(output, doc.exportHwp());
