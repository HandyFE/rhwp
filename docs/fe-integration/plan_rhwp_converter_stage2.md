# rhwp converter 고도화 계획 (2·3단계)

(rhwp converter: rhwp 를 이용한 서버용 문서편집 개발. BMS `DocConverter` 구현체를 rhwp 로 대체)

- 일자: 2026-10-29
- 기준 head: rhwp-FE `3118f9641` (1단계 `458ebdbd9` 포함)
- 선행 자료: [1단계 POC 결과](poc_rhwp_converter.md), [구조 결정 §4.1](stage3_reliability_and_server.md#41-결정-변경-2026-10-29)
- 성격: 계획. 코드를 고치지 않았다. 견적은 1인 기준 인일이며 착수 시 범위 확인 뒤 다시 보고한다.
- BMS 참조: `svnHSO_10.3_02_bms_trunk/src/main/java/com/hs/gwweb/converter/`, `appr/controller/OpenAPIProcDocC.java`·`OpenAPIRecptProcDocC.java`·`CallRemoveStampC.java`·`OpenAPIGetFieldList.java`, `appr/util/DeptAgreeUtil.java`, `appr/sign/HwpSignActionUtil.java`

## 1. 목표

BMS 에서 `docconverter.class` 를 rhwp 구현체로 바꾸면 기안·결재·접수·반려·관인 제거 처리가
한컴 변환서버·hwplib 없이 rhwp 로 동작한다. 실패하면 기존 변환기로 자동 대체되어 결재가 멈추지 않는다.

1단계는 `서명.2` 글자 서명(`insertTextEx`+`appendTextEx`)만 했다. 운영 전환에는 `DocConverter` 계약 전체와
BMS 가 실제로 만드는 액션 전부가 필요하다.

## 2. BMS 계약 대비 현재 공백

### 2.1 `DocConverter` 메서드

| 메서드 | BMS 호출처 | rhwp 1단계 | 할 일 |
|---|---|---|---|
| `convert(src, out, actions)` | 기안·결재·접수·반려·관인 제거 | 글자 2종만 | 아래 액션 표 |
| `getFieldList(src, actions)` | `OpenAPIGetFieldList`, 기안 시 필드 목록 | 없음 | W2 |
| `getFieldValues(src, actions)` | `miskey` 읽기(`DeptAgreeUtil`, 접수) | 없음 | W2 |
| `getActionMessage(actions)` | 로그용 문자열 | — | BMS 쪽 Java 에서 처리 |
| `init()` | 기동 | — | 설정 읽기, `/health` 확인 |

### 2.2 BMS 가 만드는 액션 (`converter/action/` 16종)

| BMS 액션 | 의미 | 쓰는 곳 | hwplib 처리 | rhwp 상태 |
|---|---|---|---|---|
| `insertTextEx`, `insertSignTextEx`, `insertSignDateTextEx` | 셀 필드 내용 교체 + 글꼴 | 서명·날짜·문서번호 비우기·관인 셀 비우기 | `insertCellTextByField` | **완료** |
| `appendTextEx`, `appendSignTextEx`, `appendSignDateTextEx` | 셀에 문단 추가 | 서명 아래 날짜 | `appendCellTextByField` | **완료** |
| `insertText` | 필드 값 설정(글꼴 없음). 누름틀도 대상 | 결재완료일, `miskey`, `.txt` 본문(`CELL_CBODY`) | **무시**(dispatch 에 없음). HsCell 은 `putfieldtext` | 미구현 → W3 |
| `insertImage` | 셀 비우고 그림을 셀 크기에 맞춰 삽입 | 서명 이미지(`sancmethod=1`) | `HwpLibImageUtil.IMPL_InsertPicture` | 미구현 → W4 |
| `appendPicture` | 새 문단 뒤 그림 삽입 | 날짜 위 정렬(`top`)일 때 서명 이미지 | `insertShapeWithImage(…, true)` | 미구현 → W4 |
| `addPageCell` | 을지 HWP 를 본문 뒤에 병합 | **호출처 없음**(trunk grep 0건) | `IMPL_AppendPage` | 비범위. 사용 여부 확인(§5) |
| `getFieldList` | 셀 필드 이름 목록 | 위 | hwplib 자체 순회 | W2 |
| `getTextValue` | 필드 값 읽기 | `miskey` | `getFieldValues` | W2 |

주의할 BMS 동작:
- 관인 제거(`CallRemoveStampC`)는 `insertTextEx(관인 셀, "")` 다. 셀 안의 **그림 개체까지** 지워야 한다. 1단계 `insertTextEx` 는 첫 문단 글자만 바꾼다.
- 문서번호 비우기도 `insertTextEx(CELL_DOC_NUM, "")`. 빈 값은 유효한 입력이다.
- 서명 이미지 값은 **URL**(`webip + "/bms/" + url`)이다. hwplib 는 URL 에서 내려받는다.
- 날짜 값은 `replaceFormatToDateStringWithBR` 결과로 `\r` 을 포함할 수 있다. 1단계는 문단 나눔으로 처리했고 한글 대조는 아직이다.

## 3. 작업 항목

| ID | 항목 | 내용 | 산출 위치 | 견적 |
|---|---|---|---|---|
| W1 | BMS `RhwpConverterService` | `DocConverter` 구현. `CellAction`→actions JSON, multipart 호출, `X-Rhwp-Result` 해석, 실패·503·시간 초과 시 **대체 구현체**(설정 `docconverter.rhwp.fallback.class`, 기본 hwplib) 호출, 서킷 브레이커(연속 N 실패 → T분 우회 후 반개방), `DocConvererServiceFactory` 등록, 설정 키(`docconverter.rhwp.url/token/connecttimeout/readtimeout`) | BMS SVN (커밋은 담당자) | 3~4 |
| W2 | 조회 API | `rhwp.war` `POST /fields`·`POST /field-values`, 워커 `op:"fields"/"values"`. 셀 필드만·문서 순서·같은 이름 반복 시 `{{n}}` 옵션은 BMS `GetFieldListAction.number` 와 맞춘다. CLI `fields` 가 셀 필드를 `ClickHere` 로 표기하는 결함(1단계 결함 후보 2) 수정 | rhwp-FE | 1.5~2 |
| W3 | `insertText`·빈 값·셀 비우기 | `insertText` 를 누름틀·셀 필드 공통 값 설정으로 구현(글꼴 유지). `insertTextEx("")` 는 셀의 모든 문단·그림 개체 제거 후 빈 문단 하나. 비우기 결과를 한컴 변환기 산출물과 대조 | rhwp-FE | 1~1.5 |
| W4 | 그림 삽입 | `insertImage`: 셀 비움 → 셀 안쪽 크기에 비율 유지 맞춤 → `insert_picture_native` 셀 경로 삽입. `appendPicture`: 마지막 문단 뒤 새 문단에 삽입. 크기·여백 규칙은 hwplib `HwpLibImageUtil` 과 한컴 결과를 기준으로 정한다. 이미지는 BMS 가 내려받아 multipart `image.<n>` 으로 보내고 `rhwp.war` 는 외부 네트워크를 쓰지 않는다. 1단계 `picture_test.mjs`(WASM) 로직을 Rust 로 옮긴다 | rhwp-FE, BMS | 3~4 |
| W5 | 저장 결함 1·2 수정 | `src/serializer/body_text.rs` `serialize_para_text()`. 서명 셀만 편집할 때는 걸리지 않지만 `insertText(본문 누름틀)`·결재 표 근처 셀 편집에서 재현된다. rhwp editor E3 와 같은 수정 | rhwp-FE | 2~4 |
| W6 | `verify` 보강 | 저장 후 재열기에서 셀 **전체 문단** 텍스트, 셀 안 그림 개수, 쪽수, PARA_TEXT 컨트롤 순서(`ctrlorder.py` 로직)를 비교. 기대와 다르면 `ok:false` → BMS 가 대체 경로로. 결함 2 같은 무증상 손상을 운영에서 막는 장치 | rhwp-FE | 1 |
| W7 | 회귀 게이트 | `scripts/formsweep.mjs` 정식화: 운영 서식 전체 × 시나리오(글자 서명, 이미지 서명, 날짜 top/bottom, 대결·전결, 반려, 관인 제거, 접수) → 저장 → 재열기 비교. 산출물은 담당자가 한글에서 열어 확인. 업그레이드마다 실행 | rhwp-FE `docs/fe-integration/scripts/` | 2~3 |
| W8 | 리눅스·운영 | rhwp 리눅스 빌드(glibc 버전 확인, 필요하면 musl 정적), 워커 메모리·처리 시간 실측, 운영 WAS 외부 프로세스 허용 확인, `rhwp.token`·업로드 상한, `/health` 에 대기열 길이·재시작 수·마지막 오류, 2대 구성 시 BMS URL 장애 조치, 배포 문서 | rhwp-FE `integrations/rhwp-was/` | 2~3 |
| W9 | 한글 대조 | `\r` 문단 나눔·날짜 글꼴 크기·굴림 없는 서버의 글꼴 대체를 한컴 변환기 산출물·한글 화면과 대조 | 문서 | 0.5 |
| 합계 | | | | **16~23** |

## 4. 단계 나누기 (제안)

| 단계 | 목표 | 항목 | 견적 | 완료 기준 |
|---|---|---|---|---|
| 2단계 | **글자 서명 운영 전환 가능 상태**. BMS 가 rhwp 를 쓰되 실패하면 기존 변환기로 | W1, W2, W3, W6, W8 | 8.5~11.5 | 개발 BMS 에서 `docconverter.class=RhwpConverterService` 로 기안→결재→접수→반려 한 바퀴. 워커 강제 종료·503 시 hwplib 대체 경로 동작. 리눅스 서버 실측 |
| 3단계 | **이미지 서명·관인 + 저장 안전성** | W4, W5, W7, W9 | 7.5~11.5 | 서명 이미지 계정으로 결재. 통합서식 8종 저장 결함 0. 회귀 게이트 전 서식 통과 + 한글 열기 확인 |

순서 근거: 2단계는 1단계 액션만으로 운영 경로(대체·검증·리눅스)를 먼저 닫는다. 결함 1·2 는 서명 셀만 편집할 때는
재현되지 않고, W6 이 재현 시 대체 경로로 넘긴다. 이미지 서명 사용 비율이 높거나 관인 제거가 자주 쓰이면
W4 를 2단계로 당긴다.

## 5. 착수 전 담당자에게 요청할 것

| 대상 | 요청 | 용도 |
|---|---|---|
| BMS 운영 | 하루 처리 건수·최대 동시 요청, 서버 OS·glibc 버전·Docker 여부, 운영 WAS 의 외부 프로세스 실행 허용 여부 | W8 빌드 타깃·워커 수 |
| 현재 설정 | 운영의 `docconverter.class`·`docconverter.use` 값(hwplib 인지 HsCell 인지), 서명 이미지(`sancmethod=1`) 사용 비율, 관인 사용 여부 | W1 대체 경로 기본값, W4 우선순위 |
| 을지 | `addPageCell` 사용 여부(코드상 호출처 없음) | 비범위 확정 |
| `.txt` 본문 | `insertText(CELL_CBODY)` 경로(텍스트 첨부 본문) 사용 여부 | W3 범위 |
| 정답 산출물 | 같은 서식에 한컴 변환기(또는 hwplib)로 글자 서명·이미지 서명·날짜 top·대결·관인 제거를 넣은 HWP 각 1건 | W3·W4·W9 크기·위치·문단 기준 |
| 서명·관인 이미지 | 실제 서명 이미지 PNG/JPG 샘플, 관인 이미지, 서명 이미지가 있는 계정 | W4 |
| 운영 서식 | 운영 기안·통합서식 전체 목록과 원본, 서식별 한글 Print PDF | W7 회귀 게이트 |
| 커밋 | 이슈 키 | 커밋 메시지 |

받기 전에는 추정으로 기대값을 정하지 않는다. 특히 그림 크기·위치(W4)와 셀 비우기 결과(W3)는 정답 산출물로 확정한다.

## 6. 결정이 필요한 것

| 쟁점 | 제안 |
|---|---|
| 이미지 전달 주체 | BMS 가 내려받아 multipart 로 첨부. `rhwp.war` 는 외부 URL 접근 없음(보안·장애 범위 축소) |
| 대체 경로 기본값 | hwplib(`HwpLibConverterService`). 같은 JVM 안에 있어 네트워크 의존이 없다 |
| 서킷 브레이커 | 연속 3회 실패 → 5분 우회 → 1건 시험 후 복귀. 값은 설정 키로 |
| `verify` 실패 처리 | `ok:false` 로 돌려 BMS 대체 경로. 로그에 입력 해시·액션·차이 기록 |
| 호환 유지 | `rhwp.war` 응답에 `apiVersion`. BMS 는 기동 시 `/health` 로 버전 확인 |

## 7. 리스크

| 리스크 | 대응 |
|---|---|
| 저장 결함 1·2 가 서명 외 편집에서 재현 | 2단계는 W6 검증 + 대체 경로로 막고, 3단계 W5 로 원인 수정 |
| 리눅스 글꼴(굴림) 부재로 글꼴 ID 만 기록되고 렌더링은 무관 | 저장 결과는 글꼴 이름만 쓰므로 영향 없음. PDF 출력 요구가 생기면 별도 |
| 운영 WAS 가 자식 프로세스 생성을 막음 | 전용 WAS 배치로 전환(§4.1 결정의 두 번째 배치 방식) |
| 업스트림 변경 속도 | 포크 버전 고정. W7 게이트 통과 뒤에만 동기화 |

## 8. 변경 이력

| 날짜 | 내용 |
|---|---|
| 2026-10-29 | 계획 작성. BMS `DocConverter` 계약·액션 16종 대비 공백 정리, W1~W9 와 2·3단계 제안 |
