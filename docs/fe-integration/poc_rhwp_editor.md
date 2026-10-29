# rhwp editor POC — 기안기용 에디터

(rhwp editor: rhwp 를 이용한 기안기용 에디터 개발)

- 일자: 2026-10-29
- 목적: 기안기(FeWebClient)에 rhwp 에디터가 붙은 모습 확인. 알려진 결함 수정·기능 보강은 범위 밖이다.
- 저장소
  - rhwp-FE `main`: hwpctrl 플러그인 보강, studio 임베드 빌드 스크립트
  - FeWebClient_rhwp `poc/rhwp-editor` (master 기준): `FeRhwpEditor` 추가

## 구조

```
기안기 (FeEditorFactory)
  └─ FeRhwpEditor  extends FeWebhwpEditor     ← 결재 로직은 웹한글용 코드를 그대로 씀
       └─ iframe: editor/rhwp/index.html?chrome=embed   (rhwp-studio, 같은 출처)
            └─ window.rhwpStudio.plugins → 'hwpctrl' 플러그인(@rhwp/hwpctrl)
       hwpCtrl = rhwpCtrlBridge 의 Proxy
         - 메서드 → plugins.invoke('hwpctrl','invoke',[이름,인자])  (편집은 studio 트랜잭션: undo·재조판)
         - 속성   → getProp / setProp
         - 호환 층에 없는 API → 경고 로그만 남기고 통과
```

- 웹한글은 `hwpctrlframe.html`이 `hwpCtrl`을 만들어 `buildWebHwpCtrlCallback`으로 넘긴다. POC는 같은 자리에
  rhwp 호환 객체를 넘긴다.
- `Open`은 URL을 받지 않으므로 `FeRhwpEditor.openDocument`가 내려받아 바이트로 넘긴다.
- 바이트 인자는 iframe realm의 `Uint8Array`로 복사한다. realm이 다르면 호환 층의 `instanceof` 판별이 거짓이 된다.
- 웹한글 콜백은 `{ result }` 객체를 받으므로 boolean 콜백 인자를 감싼다.

## rhwp-FE 변경

| 파일 | 내용 |
|---|---|
| `npm/hwpctrl-ocx/src/studio-plugin.mjs` | `Open`·`OpenDocument`·`Clear`는 트랜잭션 없이 호출. 교체는 studio 열기 경로에 위임되며, 문서가 없을 때 트랜잭션은 거절된다. `memberKind`·`getProp`·`setProp` 표면 추가 |
| `rhwp-studio/vite.config.ts` | `RHWP_DISABLE_PWA=1`이면 서비스 워커를 만들지 않음 |
| `rhwp-studio/package.json` | `build:fe-embed`: `--base ./`, PWA 끔, 출력 `output/fe-embed/rhwp-studio` |

## FeWebClient_rhwp 변경

| 파일 | 내용 |
|---|---|
| `src/editor/rhwp/FeRhwpEditor.js` | 에디터 클래스(`fe-rhwpeditor`) |
| `src/editor/rhwp/rhwpCtrlBridge.js` | studio 플러그인 → 웹한글 `hwpCtrl` 모양 Proxy |
| `src/editor/editorType.js`, `FeEditorFactory.js` | `isRhwpEditor()` 분기. HTML 문서가 아니면 웹한글·폴라리스보다 우선 |
| `webpack.config.fe.cjs` | studio 산출물을 `editor/rhwp/`로 복사. `RHWP_STUDIO_DIST`, 기본값 `../rhwp-FE/output/fe-embed/rhwp-studio`, 없으면 건너뜀 |

## 빌드·켜기

```bash
# 1) rhwp-FE: WASM(pkg/) 이 있는 상태에서 studio 임베드 빌드 (약 41MB)
cd rhwp-FE/rhwp-studio && npm run build:fe-embed
# 2) FeWebClient_rhwp: 빌드·배포 (dist-fe/editor/rhwp/ 에 포함됨)
yarn run deploy-fe dev
```

에디터 켜기는 서버 설정(`WebViewEditor.Url`)을 바꾸지 않는다. 다음 둘 중 하나를 쓴다.
- 기안기 주소에 `feEditor=rhwp`
- 기안기와 같은 출처의 브라우저 콘솔에서 `localStorage.setItem('fe.editor', 'rhwp')`
  - 끄려면 `localStorage.removeItem('fe.editor')`

## 확인 결과

| 항목 | 결과 | 근거 |
|---|---|---|
| 브리지 동작 (로컬 시험 페이지) | 확인 | studio iframe → 플러그인 로드 → `Open`(통합서식 1page) → `GetFieldList`·`PutFieldText`·`GetFieldText`·`PageCount` → `EditMode` 읽기·쓰기. 제목·본문 값이 화면에 그려짐 |
| 미지원 API 통과 | 확인 | `SetToolBar`, `ScrollPosInfo.Item()`: 경고만 남고 예외 없음 |
| 기안기 빌드 | 확인 | webpack 빌드 성공, eslint·prettier 통과, 새 순환 의존 없음 |
| 기안기 실제 화면 (BMS 개발 서버) | 로딩·기안 성공 (2026-10-29) | 기안기 로딩과 결재올림까지 확인했다. 도중에 멈춘 곳은 FE 브리지·`FeRhwpEditor` 에서 임시 처리했다. 저장은 `exportBytes` → Blob URL → BMS 업로드로 했다. 상신 본문을 한글로 열어 본 정합성(저장 결함 1·2)과 서명 이미지 표시는 미확인 |

POC에서 드러난 문제·개선 목록은 FeWebClient_rhwp `doc/plan/rhwp 에디터.md`에 기록한다(`poc/rhwp-editor` 브랜치).
주요 원인은 호환 층(COM 규격)과 웹한글 JS API의 차이다.
- 콜백 미지원
- `CreateSet` 속성/메서드 차이
- 모르는 액션에 `null` 반환
- realm에 따라 바이트 판별 실패

## 범위 밖 (알려진 공백)

- 저장·상신: 웹한글 `SaveAs`는 서버 업로드 후 URL을 준다. 호환 층은 바이트를 준다.
- 안 추가·이동·복사, 본문 삽입(`GetTextFile(JSON)`, `Insert`), 관인·서명, 인쇄·PDF
- 웹한글 전용 화면 API(툴바·리본·스크롤 위치 등): 경고 후 무시
- 알려진 저장 결함 1·2 ([3단계](stage3_reliability_and_server.md))
