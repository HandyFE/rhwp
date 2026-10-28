# FE 전자결재 서식 rhwp 1단계 검증 결과

- 일자: 2026-10-27
- 대상 source: rhwp v0.8.7 (`1a76570e8`, 이 포크의 `main` 기준 worktree)
- 빌드: Native `target/pr-review/release/rhwp.exe`, WASM `pkg/` (`rhwp_bg.wasm` SHA-256 `3BC0D4F9…0C0B`)
- 증적: `output/pr-review/fe-stage1/` (Git 제외 경로)

## 1. 요약

| 항목 | 결과 | 비고 |
|---|---|---|
| 서식 열기 | 충족 | FE·통합서식 16건 모두 오류 없이 열림, 전부 1쪽 |
| 한컴 PDF 대비 렌더링 | 미충족(원인 확인) | 통합서식 blank(3) 89.69% (Native·WASM 동일). 본문 안내문 출력이 원인이며 제거 시 96.04% |
| 필드 목록 | 충족 | 통합서식 누름틀·셀 필드 55개, `stamptable2` 셀 필드 `관인` 인식 |
| 필드 채우기·저장·재열기 | 충족(rhwp 기준) | 16개 채움, 저장 후 rhwp 재열기 시 값 유지·배치 정상 |
| 셀 필드에 그림 삽입 | 부분 충족 | 서명 칸·관인 표 삽입 성공. 발신명의 "글자 끝 관인" 위치 계산은 없음 |
| 한글에서 산출물 열기 | **미충족** | 3건 모두 열림. 단 `blank3_pictures` 는 하단 결재 표 전체가 사라지고, `blank3_filled` 는 결재 표가 `본문` 누름틀 안으로 들어감. §7 |
| 감사 서식 표 배치 | 미검증 | rhwp가 `LAYOUT_TABLE_OVERLAP`(123.8px) 경고. 기준 PDF 없음 |

## 2. 입력

| 파일(작업명) | 원본 | 쪽 | 표 | 필드 |
|---|---|---|---|---|
| tonghap_blank3 | `D:\fe_works\feTempSpace\[웹한글] 통합서식 blank (3).hwp` + 한컴 PDF(Hwp 2018 10.0.0.11529, PDF 1.4, A4 1쪽) | 1 | 2 | 55 |
| tonghap_blank1 / 1page / audit / linked / fixedrecv / 3to4 / 5to6 | 같은 폴더의 통합서식 변형 | 1 | 2~3 | 40~74 |
| fe_stamptable2(_en_US) / AuditStamp / ControlStamp / AuditComment / Summary | `FeWebClient/src/res/*.hwp` | 1 | 0~2 | 1~4 |
| fe_attachini_body(.hwp/.hwpx) | `FeWebClient/test/attachini/` | 1 | 0 | 0 |

`doc/reference/hox/*` 는 본문 HWP를 경로로만 참조하므로 실제 기안 본문 샘플은 위 통합서식뿐이다.

## 3. 렌더링 (한컴 PDF 대비)

명령: `scripts/visual_sweep.py --hwp … --pdf … --dpi 96` (Native 전체, Native/WASM `--silhouette-only`)

| 경로 | 쪽 | 2px 관용 실루엣 일치율 | 90% 미만 |
|---|---|---|---|
| Native | 1/1 (PDF 1쪽) | 89.68960% | p1 |
| fresh WASM | 1/1 | 89.68960% | p1 |
| 진단: `본문` 값을 비운 사본(Native) | 1/1 | 96.03719% | 없음 |

- 이진화 원값 = 관용값(경계 대조 픽셀 0). 실루엣 일치율은 보조값이며 전체 정확도가 아니다.
- 직접 판독(`native/tonghap_blank3/review/review_001.png`): 수신·경유·제목 줄, 제목 아래 선,
  회색 구분 막대, 협조자·시행·우·전화 줄의 위치는 PDF와 일치. 하단 소형 글자 2px 내외 차이.
- **결함 후보 1 — 누름틀 안내문 인쇄**: `본문` 누름틀 command가 `"본문을 입력하세요.;"`
  (웹한글이 만든 것으로 보이는 형식)이라 `Field::guide_text()`(`src/model/control.rs`)가
  `Direction:wstring:` 형식만 인식해 안내문으로 보지 못한다. 그래서 안내문 글자를 값으로
  취급해 인쇄 프로필에서도 그린다(한컴 PDF에는 없음). `fields` 결과도 `value`에 안내문이 들어간다.
  → FE `GetFieldText('본문')`이 빈 값 대신 안내문을 돌려줄 위험이 있다.

## 4. 필드

- CLI `fields` 와 WASM `getFieldList` 모두 55개를 반환한다. FE 핵심 이름 `본문`, `수신`, `경유`, `결재제목`,
  `발신명의`, `직위.1~8`, `서명.1~8`, `협조.1~5`, `문서번호`, `시행일자` 등을 모두 확인했다.
- `서명.N`, `발신명의`, `관인` 은 **셀 필드**(셀에 붙인 이름)다. WASM은 `cellField:true`,
  `location.path[{type:"cell",controlIndex,cellIndex,paraIndex}]` 로 정확히 보고한다.
  **결함 후보 2**: CLI `fields` 는 같은 필드를 `fieldType:"ClickHere"` 로 표기하고 좌표 키 이름도
  WASM과 다르다(`nested[{kind:"tableCell",control,cell}]`). 서버(CLI)와 브라우저(WASM)의 스키마를 통일해야 한다.
- `edit fill-fields` 로 16개 필드를 채웠다(`notFound`·`ambiguous` 0). 서식 편집 불가 필드도 채워진다.
  저장 후 다시 열면 값이 유지되고, 렌더링(`filled.png`)에서 제목·본문·발신명의·결재란 이름·시행 정보가
  각 칸에 표시된다. 본문의 `\n` 은 문단으로 나뉜다(둘째 줄은 들여쓰기 없음, 한컴 비교는 미검증).

## 5. 그림(서명·관인) 삽입

방법: Node + fresh WASM `insertPictureEx` (`cellPath` 지정). 필드명으로 셀을 찾고
`getCellProperties` 의 셀 크기에 비율을 유지해 맞췄다. 스크립트는 [`scripts/picture_test.mjs`](scripts/picture_test.mjs).

| 대상 | 셀 크기(HU) | 그림(HU) | 결과 |
|---|---|---|---|
| 통합서식 `서명.1`, `서명.2` | 4620×2198 | 3906×1562 | 성공, 서명 칸 안 표시 |
| 통합서식 `발신명의` | 35083×4042 | 3638×3638 | 성공. 단 셀 **왼쪽 위**에 놓임(FE는 발신명의 글자 끝에 겹쳐 찍음) |
| `stamptable2` `관인` | 17645×14895 | 14895×14895 | 성공, 관인 표에 맞게 표시 |

- 셀 그림은 셀 안 글자처럼 들어가지 않는다. 표와 같은 층에 떠 있는 개체(floating, #1151)로 들어가며 `paperOffsetX/YHu` 로 위치를 정한다.
- CLI `edit insert-picture` 에는 셀 경로 인자가 없다. 서버에서 하려면 WASM(Node) 경로나 새 CLI가 필요하다.
- 저장 후 rhwp 재열기: 쪽수 1, 필드 값 유지.

### 한컴 호환성 확인용 산출물 (직접 열어 확인 필요)

`output/pr-review/fe-stage1/hancom-check/`

- `blank3_filled.hwp` — 필드 16개 채움(CLI)
- `blank3_pictures.hwp` — 서명 2개 + 발신명의 관인(WASM)
- `stamptable2_picture.hwp` — 관인 표에 관인(WASM)

확인할 점: 한글에서 열림 여부, 누름틀 이름·값 유지, 그림 위치·크기, 다시 저장 가능 여부, 웹한글 `GetFieldText`.

## 6. 2단계(API 범위·견적)에 반영할 사항

1. 누름틀 안내문 형식(`"텍스트;"`) 인식: 렌더링(인쇄 제외)과 필드 값 판정에 필요.
2. 필드 스키마 통일(CLI ↔ WASM), 셀 필드/누름틀 구분 노출.
3. `InsertPicture` 호환: 필드명 → 셀 → 크기 맞춤, "발신명의 글자 끝" 같은 기준점 배치 옵션.
4. 서버용 CLI(또는 Node 서비스): `insert-picture` 에 필드명·셀 경로 인자 추가.
5. 한컴/웹한글 왕복 검증 결과에 따라 직렬화 보강 범위를 정한다.

## 7. 한글 확인 결과와 원인 (2026-10-27 추가)

사용자가 한글(데스크톱)에서 §5 산출물 3건을 직접 열어 확인했다.

| 파일 | 한글 표시 | 판정 |
|---|---|---|
| `stamptable2_picture.hwp` | 관인 그림이 관인 칸에 정상 표시 | 충족 |
| `blank3_filled.hwp` | 열림. 수신·제목·본문 2줄·발신명의·결재란 이름·시행 정보 표시. 단 조판부호에서 `[누름틀 시작]본문…[표][누름틀 끝]` — 하단 결재 표가 `본문` 누름틀 **안**으로 들어감 | 미충족 |
| `blank3_pictures.hwp` | 열림. 상단 표와 빈 `본문` 누름틀만 보이고 **하단 결재 표 전체(발신명의·직위·서명 칸)와 그림 3개가 표시되지 않음** | 미충족 |

### 원인: 문단 재직렬화 때 누름틀 끝 표시가 뒤 개체들 뒤로 밀림

첫 문단(이 서식은 본문 전체가 한 문단)의 PARA_TEXT 순서를 직접 읽었다
(스크립트 [`scripts/paratext.py`](scripts/paratext.py), 증적 `output/pr-review/fe-stage1/diag-hancom/`).

```text
원본      [secd][cold][tbl ] ␣×10 [%clk 시작]본문을 입력하세요.[clk 끝][tbl ][문단끝]
filled    [secd][cold][tbl ] ␣×10 [%clk 시작]1. 관련…[줄바꿈]2. 위와…[tbl ][clk 끝][문단끝]
pictures  [secd][cold][tbl ] ␣×10 [%clk 시작]본문을 입력하세요.[tbl ][gso ][gso ][gso ][clk 끝][문단끝]
```

- 원본은 `본문` 누름틀이 글자만 감싸고, 하단 결재 표(`tbl`)는 누름틀 **밖**에 있다.
- rhwp가 이 문단을 다시 쓰면(값 채우기·그림 추가로 문단이 바뀐 경우) 누름틀 끝 표시를 남은
  개체들 **뒤**에 쓴다. 그래서 결재 표와 새 그림이 `본문` 누름틀 안으로 들어간다.
- 변경 없이 열고 저장한 사본(`blank3_roundtrip_wasm.hwp`)은 원본 순서를 유지한다 — 문단이 바뀔 때만 생긴다.
- `pictures` 는 `본문` 누름틀의 '수정됨' 비트(속성 bit 15)가 0이다. 한글은 수정되지 않은 누름틀의 내용을
  안내문 상태로 다루므로, 그 안에 들어간 결재 표·그림까지 함께 표시하지 않은 것으로 보인다.
  `filled` 는 값을 채우며 비트가 1이 되어 표가 보였지만, 표가 누름틀 안에 있다.
- 코드 위치: `src/serializer/body_text.rs` `serialize_para_text()`. 텍스트 끝에서 닫히는 필드
  (`end_char_idx == text_len`)의 끝 표시는 `trailing_end_after_ctrl[control_idx + inner_slot_count]` 로
  예약된다. 그런데 `본문` 필드의 시작 컨트롤은 본문 루프(③ FIELD_BEGIN 강제 방출)에서 이미 나가므로
  뒤쪽 남은 컨트롤 루프에서 키를 만나지 못한다. 결국 맨 끝의 고아 처리(`trailing_end_after_ctrl.values()`)로
  모든 컨트롤 뒤에 쓰인다.

### 영향

- FE의 결재 서식은 `본문` 누름틀 뒤에 결재 표를 두는 구조라, rhwp에서 값을 채우거나 서명·관인을 넣고 저장하면
  **한글에서 결재 표가 사라지거나 본문 누름틀에 묶인다.** 웹한글 `PutFieldText('본문', …)` 가 결재 표를 지울 위험도 있다.
- 서버(BMS)에서 rhwp로 서명·관인을 넣는 시나리오도 같은 저장 경로를 쓰므로 같은 결함에 걸린다.
- 2단계 견적의 **최우선 선행 수정**으로 분류한다.

### 결함 후보 1 보강

`clear_initial_field_texts()`(`src/document_core/commands/document.rs`)는 수정되지 않은 누름틀의 내용이
안내문과 같으면 빈 값으로 정규화한다. 이 서식은 command가 `"본문을 입력하세요.;"` 라
`Field::guide_text()` 가 안내문을 얻지 못해 정규화가 일어나지 않았다(§3과 같은 원인).

## 8. 환경 메모 (Windows 재현)

- Rust 1.93.1(rustup, `rust-toolchain.toml` 고정), MSVC Build Tools 2019, wasm-pack 0.15.0(공식 릴리스 바이너리, CI 고정 버전),
  poppler 25.07(winget), Node 22.20, `npm --prefix rhwp-studio ci`.
- Native release 빌드는 처음 약 13분, 그 뒤 WASM 빌드를 이어서 실행.
- `visual_sweep.py` 실행에는 `PYTHONUTF8=1`, `VISUAL_SWEEP_CHROME=<chrome.exe>`, poppler bin PATH가 필요(cp949 디코딩 오류 회피).
- `scripts/wasm-pack-locked.ps1` 은 `.sh` 와 달리 `rhwp-studio/public/` 동기화를 하지 않는다. 그래서 수동 복사하고 SHA-256 일치를 확인했다.
  (이 때문에 추적 파일 `rhwp-studio/public/rhwp.js` 가 수정 상태다.)
