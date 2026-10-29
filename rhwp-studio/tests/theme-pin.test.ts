import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));

// URL `?theme=` 고정 규칙은 두 곳에 있다 — 첫 페인트용 theme-init.js 와 런타임 theme.ts.
// 둘이 같은 값을 받아들이는지 소스 문자열로 확인한다(theme.ts 는 DOM 의존이라 직접 import 하지 않는다).
test('theme-init.js 는 URL theme 값을 저장 설정보다 우선한다', () => {
  const src = readFileSync(join(here, '../public/theme-init.js'), 'utf8');
  assert.match(src, /URLSearchParams\(window\.location\.search\)\.get\('theme'\)/);
  const pin = src.indexOf('if (pinnedMode) mode = pinnedMode;');
  const effective = src.indexOf('const effective =');
  assert.ok(pin >= 0 && effective > pin, '고정값 적용은 effective 계산 앞이어야 한다');
});

test('theme.ts 의 getPinnedThemeMode 는 light|dark|system 만 받는다', () => {
  const src = readFileSync(join(here, '../src/core/theme.ts'), 'utf8');
  assert.match(src, /export function getPinnedThemeMode\(/);
  assert.match(src, /raw === 'light' \|\| raw === 'dark' \|\| raw === 'system' \? raw : null/);
  assert.match(src, /return getPinnedThemeMode\(\) \?\? userSettings\.getThemeSettings\(\)\.mode;/);
});
