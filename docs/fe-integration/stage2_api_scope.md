# FE 전자결재 편집기 rhwp 전환 2단계: API 우선순위와 구현 범위·견적

- 일자: 2026-10-27
- 기준 source: rhwp-FE v0.8.7 (`1a76570e8`), FeWebClient 현재 작업 트리
- 선행 자료: [1단계 검증 결과](stage1_verification.md) (특히 §7 한글 확인 결과)
- 성격: 착수 전 범위·견적 보고. 이 문서 작성 과정에서 코드를 고치지 않았다.

## 1. 결론 요약

1. **선행 결함 하나가 모든 시나리오를 막는다.** rhwp가 `본문` 누름틀이 든 문단을 다시 저장하면 누름틀 끝 표시를
   뒤 개체들 뒤로 옮긴다. 그 결과 결재 표가 `본문` 누름틀 안으로 들어가거나, 한글에서 결재 표·서명·관인이 통째로
   사라진다(1단계 §7). 편집기 교체와 서버 처리 모두 이 저장 경로를 쓰므로 가장 먼저 고친다.
2. **편집기 호환 층이 이미 있다.** 포크에 웹한글컨트롤 API 호환 패키지 `npm/hwpctrl-ocx`(`@rhwp/hwpctrl`)가 있고,
   원장 484항목 중 339항목이 완료(verified 333, substituted 6)다. rhwp-studio 위에 플러그인으로 얹어 화면까지 그린다.
   FE가 쓰는 직접 메서드·속성 47종 중 **33종이 verified**다(substituted 2, unimplemented 11, 원장 없음 1).
   단 `GetTextFile`·`SetTextFile`·`SaveAs` 는 verified여도 FE가 쓰는 형식(JSON·saveblock·insertfile·PDF)은 다루지 않는다.
3. **남은 공백은 "블록 직렬화"와 "Action 13종"에 몰려 있다.** FE는 안 추가·이동·복사, 서식 변환, 셀 복사,
   본문 삽입, 관인 표 삽입을 모두 `GetTextFile(JSON, saveblock)` → `SetTextFile/Insert(insertfile)` 와
   `InsertFile` 액션으로 처리한다. 호환 층은 이 경로를 텍스트로만 흉내 내거나 아직 구현하지 않았다.
   이 부분이 가장 크고 불확실하다.
4. **서버(BMS) 서명·관인·필드 채우기는 따로 먼저 끝낼 수 있다.** 선행 결함 수정 + CLI 명령 추가 정도로 가능하다.

| 단계 | 내용 | 견적(1인, 인일) | 확신도 |
|---|---|---|---|
| A. 선행 결함 수정 | 누름틀 끝 순서, 안내문 형식, CLI 필드 스키마 | 5~7 | 높음 |
| B. 서버 처리(BMS) | 필드 채우기·서명/관인 삽입·PDF CLI, 리눅스 빌드 | 6~9 | 중간 |
| C. 편집기 교체 | 호환 층 공백 메우기, 프레임 교체, FE 수정, 왕복 검증 | 50~75 | 낮음~중간 |
| **합계** | | **61~91** (약 3~4.5인월) | |

A+B만 먼저 진행하면 약 **2.5~3.5주**에 "편집기는 웹한글 유지, 서버 처리는 rhwp" 상태를 만들 수 있다.

## 2. FE가 쓰는 웹한글 API와 rhwp 대응

조사 범위: `src/editor/FeEditor.js`, `src/editor/hwp/FeWebhwpCtrl.js`, `src/editor/hwp/FeWebhwpEditor.js` 와
래퍼를 쓰는 `src/utils/SignCellSplitterHwp.js`, `src/hie/hie.js`, `src/qdb/*`.
상태는 `npm/hwpctrl-ocx/spec/api_ledger.json` 기준이다(verified = 한글 2022 COM Oracle과 0 diff).

분류 기준:
- **필수**: 결재 핵심 흐름(열기·필드·서명·관인·안 관리·저장)에 그대로 동작해야 한다.
- **대체 가능**: FE 쪽 구현을 바꾸거나 rhwp의 다른 기능으로 같은 결과를 낼 수 있다.
- **불필요**: 웹한글 화면 장식이거나 호출하는 곳이 없다.

### 2.1 직접 메서드·속성

| API | FE 용도 | 원장 상태 | 분류 | 남은 일 |
|---|---|---|---|---|
| `GetFieldText` / `PutFieldText` (`\x02` 일괄) | 필드 값 읽기·쓰기 | verified | 필수 | 선행 결함 A1 수정 후 한글 왕복 재확인 |
| `GetFieldList(fmt,type)` / `FieldExist` | 필드 목록·개수 | verified | 필수 | 셀/누름틀 분리 조회 동작 확인 |
| `MoveToField` | 커서 편집의 출발점 | verified | 필수 | — |
| `MoveToFieldEx` | 이동 + 화면 스크롤 | unimplemented | 대체 가능 | `MoveToField` + studio 스크롤(0.5일) |
| `MovePos` (2·5·23·24) | 상위 리스트·줄 끝·리스트 끝·문서 처음 | verified | 필수 | — |
| `SelectText` / `GetSelectedPos` | 블록 지정, 빈 본문 판정 | verified | 필수 | — |
| `GetCurFieldName` / `SetCurFieldName` | 현재 안 판정, 새 셀 이름 | verified | 필수 | — |
| `RenameField` / `ModifyFieldProperties` / `CreateField` | 안 번호 재지정, 누름틀 생성 | verified | 필수 | — |
| `InsertPicture` (sizeOpt 0/1/3, 콜백 ctrl) | 서명·관인·로고·본문 이미지 | verified | 필수 | 콜백 ctrl의 `Properties` 대입(TreatAsChar·TextWrap·Lock) 확인 |
| `InsertCtrl('%hlk')` | 하이퍼링크 | verified | 필수 | — |
| `DeleteCtrl` / `ParentCtrl` (`CtrlID`) | 관인 표 삭제, 표 안 판정 | verified | 필수 | — |
| `Open` | 문서·서식 열기 | verified | 필수 | URL 입력은 호스트 고리 `onReadFile` 로 연결 |
| `SaveAs` HWP/HWPX | 결재 본문 서버 저장 | verified | 필수 | 웹한글은 자기 서버에 올리고 `downloadUrl` 을 준다. rhwp는 바이트를 주므로 FE `saveServer` 가 직접 업로드하도록 변경 |
| `SaveAs` PDF | PDF 저장·인쇄 대용 | 미지원(HWP/HWPX/HML만) | 대체 가능 | 서버 CLI `export-pdf` 또는 studio 인쇄 |
| `SaveAs` PUBDOCBODY | 공문서 본문 XML | 미지원 | 필수(공문서 유통 시) | **형식 조사 필요**. 견적 별도 |
| `GetTextFile` JSON/HWP + `saveblock` | 블록 복사(안·셀·본문·관인) | verified(TEXT/UNICODE만). JSON·saveblock 블록 내용은 구현 없음 | 필수 | **블록 직렬화(§3 C3)** |
| `SetTextFile` JSON/HTML/TEXT + `insertfile` | 블록 붙여넣기 | verified(문서 맨 앞에 텍스트만) | 필수 | **블록 삽입(§3 C3)** |
| `Insert` HWP/HWPX/HTML/JSON/TEXT | 캐럿 위치 문서 삽입(본문 삽입) | unimplemented | 필수 | **문서 끼워넣기(§3 C3)** |
| `CreatePageImageEx` | 발송용 쪽 이미지 | unimplemented | 대체 가능 | canvas 렌더 → PNG(2일) |
| `PrintDocument` | 배포용 문서 인쇄 | unimplemented | 대체 가능 | studio 인쇄 경로 연결(1~2일) |
| `EditMode` (0/1/2/16) | 읽기 전용·양식 모드 | verified(값만 보관, 제약 없음) | 필수 | 읽기 전용 신규, 양식 모드 제약을 studio 입력 층에 연결(3~4일) |
| `IsModified` | 변경 여부 저장·복원 | substituted | 필수 | FE가 값을 직접 대입하는 사용 방식과 맞는지 확인 |
| `CharShape` / `CellShape` (get) | 관인 위치·서명 분할 계산 | verified | 필수 | — |
| `ParaShape` (get/set) | 정렬 | verified | 불필요 | 호출하는 곳 없음 |
| `ViewProperties` (조판부호·배율) | 편집 중 조판부호 표시 | verified | 대체 가능 | studio 보기 설정에 연결 |
| `PageCount` / `CurFieldState` / `Clear` | 쪽 수, 셀/누름틀 분기, 문서 비우기 | verified | 필수 | — |
| `GetTableCellAddr` | 서명 셀 분할 좌표 | unimplemented | 필수 | 셀 주소 조회(0.5일) |
| `GetMousePos` + `AddEventListener(0)` | 수동 날인 클릭 좌표, 클릭한 안 선택 | unimplemented | 필수 | studio 마우스 이벤트 → hitTest 연결(2일) |
| `ScrollPosInfo` | 초기 캐럿 스크롤 | unimplemented | 대체 가능 | studio 스크롤 API(1일) |
| `LockCommand('ShapeObjSaveAsPicture')` | 서명 그림 저장 방지 | verified | 대체 가능 | studio에 해당 메뉴가 없으면 무시 |
| `ShowCaret` / `SetToolBar` / `FoldRibbon` / `ShowStatusBar` | 화면 장식 | unimplemented / 원장 없음 | 불필요 | studio `chrome.set` 으로 대응 |
| `Version` | 비교기 선택 | substituted | 대체 가능 | FE 비교기 분기를 rhwp 값에 맞춤 |
| `CreateAction` / `CreateSet` / `Run` / ParameterSet | 액션 실행 틀 | verified | 필수 | 아래 개별 액션 참고 |

### 2.2 Action

| Action | FE 용도 | 원장 상태 | 분류 | 남은 일(인일) |
|---|---|---|---|---|
| `Cancel`, `Erase`, `BreakPara`, `BreakPage`, `MoveDocEnd`, `MovePageUp`, `SelectAll`, `DeleteBack`, `InsertSpace` | 공통 편집 | verified | 필수 | — |
| `ParagraphShapeAlignCenter/Left/Right` | 서명 정렬 | verified | 필수 | — |
| `TableCellBlock(Extend)`, `TableRight/Left/Upper/LowerCell`, `TableMergeCell`, `TableDeleteCell`, `TableInsertLowerRow` | 서명 셀 분할·병합, 행 추가 | verified | 필수 | — |
| `DocumentInfo` | **안 경계·커서 좌표·빈 문서 판정(매우 많이 씀)** | unimplemented | 필수 | `CurPara/CurPos/RootPara/RootPos/Detail*` 값 의미 맞추기(2~3) |
| `TablePropertyDialog` | 관인 표 위치 조회·설정 | unimplemented | 필수 | `get/setTableProperties` 매핑(2~3) |
| `InsertFile` | 관인 표·문서 끼워넣기 | unimplemented | 필수 | §3 C3에 포함 |
| `InsertText` | 서명 이름·글자 입력 | unimplemented | 필수 | `insertText` 매핑(0.5) |
| `CharShape` | 서명 글자 모양 | unimplemented | 필수 | `applyCharFormat` 매핑(1) |
| `ParagraphShape` | 정렬 조회 | unimplemented | 필수 | `getParaPropertiesAt` 매핑(0.5) |
| `TableSplitCell`, `TableDistributeCellWidth/Height` | 서명 셀 분할 | unimplemented | 필수 | `splitTableCellInto`, 셀 균등 분배(2~3) |
| `CellBorder` (대각선) | 빈 칸 사선 | unimplemented | 필수 | `setCellProperties` 대각선(1) |
| `AllReplace` | 개인정보 치환 | unimplemented | 필수 | `replaceAll` 매핑(1) |
| `InsertHyperlink` | 관련문서 링크 | unimplemented | 필수 | `insertHyperlinkEx` 매핑(1) |
| `FileSetSecurity` | 배포용 문서 생성 | unimplemented | 필수(배포용 사용 시) | **rhwp가 배포용 문서를 저장할 수 있는지 조사 필요**. 견적 별도 |
| `ViewZoom` | 보기 배율 | 원장 없음 | 대체 가능 | studio 배율 API(0.5) |

## 3. 작업 범위와 견적

견적은 rhwp 코드베이스에 익숙한 개발자 1인 기준이다. 검증 시간(정식 회귀 테스트, CLAUDE.md의
Native/fresh WASM Visual Sweep, 한글 왕복 확인)을 포함했다.

### A. 선행 결함 수정 (5~7일, 확신도 높음)

| 번호 | 내용 | 근거·위치 | 견적 |
|---|---|---|---|
| A1 | 누름틀 끝 표시를 뒤 개체 앞에 유지. 텍스트 끝에서 닫히는 필드의 시작 컨트롤이 본문 루프에서 이미 나간 경우, 남은 컨트롤을 쓰기 **전에** 끝 표시를 쓴다 | `src/serializer/body_text.rs` `serialize_para_text()` 의 `trailing_end_after_ctrl` | 2~3 |
| A2 | `"텍스트;"` 형식 누름틀 command의 안내문 인식 → 인쇄 제외, 값 API는 빈 값 | `Field::guide_text()`, `clear_initial_field_texts()` | 1.5~2 |
| A3 | CLI `fields` 가 셀 필드를 셀 필드로 표기하고 좌표 키를 WASM과 맞춤 | CLI fields 출력 | 1 |
| A4 | 한글 왕복 재확인(1단계 산출물 3종 재생성) | 사용자 확인 필요 | 0.5 |

- A1은 원작 저장소(upstream)에도 있을 가능성이 높다. 다만 rhwp-FE는 핸디 기안기용 포크이고 당분간 upstream에 PR을 내지 않으므로(2026-10-28 결정), 이 포크에서 고친다.
- A2는 command `"본문을 입력하세요.;"` 의 의미를 독립 근거(한컴 문서·정상 생성본)로 먼저 확정해야 한다.

### B. 서버 처리(BMS, 리눅스 Java) (6~9일, 확신도 중간)

| 번호 | 내용 | 견적 |
|---|---|---|
| B1 | CLI `edit insert-picture-at-field`: 필드명 → 셀 경로 → 셀 안쪽 크기에 비율 맞춤. 셀 그림 저장 형식 확인 | 2~3 |
| B2 | 기준점 배치 옵션: "발신명의 글자 끝에 겹침"(FE `putSealStampToCell` 계산 이식) | 2~3 |
| B3 | 리눅스 x86_64 CLI 빌드·배포 방식(단일 바이너리), Java `ProcessBuilder` 호출 규약(JSON 입출력) | 1~1.5 |
| B4 | 서버 PDF: CLI `export-pdf`(svg backend), 서버 글꼴 공급(`RHWP_FONT_PATH`) 확인 | 1~1.5 |

### C. 편집기 교체 (50~75일, 확신도 낮음~중간)

| 번호 | 내용 | 견적 | 위험 |
|---|---|---|---|
| C1 | 프레임 교체: `hwpctrlframe.html` 대신 rhwp-studio + `@rhwp/hwpctrl` 플러그인을 같은 출처 iframe에 올리고, `BuildWebHwpCtrl` 콜백 계약과 동기 호출을 유지 | 5~8 | 중 |
| C2 | 미구현 메서드·Action(§2의 "남은 일" 합계) | 15~20 | 중 |
| C3 | **블록 직렬화·문서 끼워넣기**: `GetTextFile(saveblock)` 블록 추출, `SetTextFile/Insert(insertfile)` 캐럿·셀 위치 삽입, `InsertFile`(KeepSection/Style/Charshape/Parashape), 문서 사이 글자·문단·테두리 모양 ID 재매핑 | 12~20 | **높음** |
| C4 | `EditMode` 읽기 전용·양식 모드 제약, `IsModified` 의미 | 3~4 | 중 |
| C5 | FE 수정: `saveServer` 업로드 경로, PDF·쪽 이미지·인쇄 대체, `Version` 분기, 새 편집기 클래스 등록 | 5~8 | 중 |
| C6 | 결재 시나리오 E2E 검증: 기안 → 결재 → 서명 → 관인 → 안 추가·이동 → 서식 변환 → 발송. 각 산출물 한글·웹한글 왕복 | 8~12 | 중 |
| C7 | 조사 후 견적: `SaveAs PUBDOCBODY`(공문서 XML), `FileSetSecurity`(배포용 저장) | 조사 2~3 | 미정 |

C3가 견적 폭을 가장 크게 만든다. FE가 `JSON` 문자열을 같은 편집기 사이에서만 주고받는다면 rhwp 고유 형식
(문서 조각 HWP 바이트 등)으로 정의할 수 있어 하한에 가깝다. 이미 서버에 저장된 웹한글 JSON을 다시 읽어야 한다면
웹한글 JSON 형식 해석이 추가로 필요해 상한을 넘을 수 있다.

## 4. 확인이 필요한 사항

1. FE가 `GetTextFile('JSON')` 결과를 서버나 DB에 **저장**하는가(자동저장·임시저장 등), 아니면 편집기 사이 전달에만 쓰는가.
2. 공문서 유통(`PUBDOCBODY`)과 배포용 문서(`FileSetSecurity`)를 운영에서 실제로 쓰는가.
3. 웹한글을 쓰는 다른 사용처(고객 스크립트 `sanc0_support.js` 의 `GetControl()` 노출 등)를 지원 범위에 넣는가.
4. 편집기 교체를 전 서식에 한 번에 할지, 서버 처리(A+B)를 먼저 운영에 넣고 편집기는 단계적으로 할지.

## 5. 권장 진행 순서

1. A(선행 결함) → 한글 왕복 재확인.
2. B(서버 처리)를 먼저 운영 후보로 만든다. 편집기는 웹한글을 그대로 둔다.
3. C는 C7 조사와 §4 확인 뒤 견적을 다시 좁히고, C1·C3 시제품(1~2주)으로 위험부터 확인한다.

## 6. 참고: FE 코드에서 함께 발견한 결함 (rhwp와 무관, 수정하지 않음)

- `FeEditor.js` `#copyStamp`: `movePos()` 가 값을 반환하지 않아 `if (!targetEditor.movePos(24))` 가 늘 참 → 관인 복사가 실행되지 않음.
- 같은 함수: `stampStart.curpara/curpos` 소문자 키 사용(`getDocumentInfo` 는 `CurPara/CurPos`).
- `FeWebhwpEditor.js` `doManualSealStamp`: 끝난 뒤 안 선택 마우스 리스너를 다시 등록하지 않음.
- `FeEditor.js` `clearSign`: 대상 필드로 이동하지 않고 정렬 액션을 실행.
