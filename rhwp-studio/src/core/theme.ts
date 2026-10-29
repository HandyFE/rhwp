import { userSettings, type ThemeMode, type ThemeSkin } from './user-settings';

export type EffectiveTheme = 'light' | 'dark';

const THEME_QUERY = '(prefers-color-scheme: dark)';
const THEME_COLOR_META_SELECTOR = 'meta[name="theme-color"]';
const COLOR_SCHEME_META_SELECTOR = 'meta[name="color-scheme"]';

function prefersDark(): boolean {
  return window.matchMedia?.(THEME_QUERY).matches ?? false;
}

function syncThemeColorMeta(root: HTMLElement): void {
  const meta = document.querySelector<HTMLMetaElement>(THEME_COLOR_META_SELECTOR);
  if (!meta) return;
  const themeColor = getComputedStyle(root).getPropertyValue('--ui-bg-light').trim() || '#f5f5f5';
  meta.content = themeColor;
}

function syncBrowserColorScheme(root: HTMLElement, effective: EffectiveTheme): void {
  const scheme = `only ${effective}`;
  root.style.colorScheme = scheme;
  const meta = document.querySelector<HTMLMetaElement>(COLOR_SCHEME_META_SELECTOR);
  if (meta) meta.content = scheme;
}

/**
 * URL `?theme=light|dark|system` 로 고정한 테마 모드. 호스트가 임베드(`chrome=embed`)할 때
 * 저장된 사용자 설정·시스템 테마와 무관하게 테마를 고정하는 용도다(기안기는 `theme=light`).
 * 없거나 값이 잘못되면 `null`. `public/theme-init.js` 의 첫 페인트 적용과 같은 규칙이다.
 */
export function getPinnedThemeMode(search: string = window.location.search): ThemeMode | null {
  const raw = new URLSearchParams(search).get('theme')?.trim().toLowerCase();
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : null;
}

export function getThemeMode(): ThemeMode {
  return getPinnedThemeMode() ?? userSettings.getThemeSettings().mode;
}

export function getEffectiveTheme(mode: ThemeMode = getThemeMode()): EffectiveTheme {
  if (mode === 'dark') return 'dark';
  if (mode === 'light') return 'light';
  return prefersDark() ? 'dark' : 'light';
}

export function getThemeSkin(): ThemeSkin {
  return userSettings.getThemeSettings().skin;
}

export function applyTheme(mode: ThemeMode = getThemeMode()): EffectiveTheme {
  const effective = getEffectiveTheme(mode);
  const root = document.documentElement;
  root.dataset.themeMode = mode;
  root.dataset.themeEffective = effective;
  applyThemeSkin(root);
  syncBrowserColorScheme(root, effective);
  syncThemeColorMeta(root);
  return effective;
}

/** 스킨을 root data 속성으로 반영한다. 기본 스킨은 속성을 제거해 CSS 미적용 상태로 둔다. */
function applyThemeSkin(root: HTMLElement, skin: ThemeSkin = getThemeSkin()): void {
  if (skin === 'default') delete root.dataset.themeSkin;
  else root.dataset.themeSkin = skin;
}

export function setThemeMode(mode: ThemeMode): EffectiveTheme {
  userSettings.setThemeMode(mode);
  // URL 로 고정된 모드가 있으면 설정은 저장하되 화면은 고정값을 유지한다.
  return applyTheme(getPinnedThemeMode() ?? mode);
}

export function setThemeSkin(skin: ThemeSkin): EffectiveTheme {
  userSettings.setThemeSkin(skin);
  // 스킨 변경 후 theme-color meta 등 파생 값도 함께 갱신한다.
  return applyTheme();
}

export function syncThemeMenu(mode: ThemeMode = getThemeMode(), skin: ThemeSkin = getThemeSkin()): void {
  for (const item of document.querySelectorAll<HTMLElement>('[data-theme-mode-choice]')) {
    const active = item.dataset.themeModeChoice === mode;
    item.classList.toggle('active', active);
    item.setAttribute('aria-checked', String(active));
  }
  for (const item of document.querySelectorAll<HTMLElement>('[data-theme-skin-choice]')) {
    const active = item.dataset.themeSkinChoice === skin;
    item.classList.toggle('active', active);
    item.setAttribute('aria-checked', String(active));
  }
}

export function initThemeSync(onChange?: (effective: EffectiveTheme, mode: ThemeMode) => void): () => void {
  const notify = () => {
    const mode = getThemeMode();
    const effective = applyTheme(mode);
    syncThemeMenu(mode);
    onChange?.(effective, mode);
  };

  notify();

  const media = window.matchMedia?.(THEME_QUERY);
  if (!media) return () => {};

  const onMediaChange = () => {
    if (getThemeMode() === 'system') notify();
  };
  if (typeof media.addEventListener === 'function') {
    media.addEventListener('change', onMediaChange);
    return () => media.removeEventListener('change', onMediaChange);
  }
  media.addListener(onMediaChange);
  return () => media.removeListener(onMediaChange);
}
