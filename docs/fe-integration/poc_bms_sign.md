# BMS HWP 수정툴 POC 1단계 — `서명.2` 글자 서명

- 일자: 2026-10-29
- 목적: BMS 서버에서 UI 없이 결재 본문 HWP 의 서명 셀에 글자 서명을 넣는다. 한컴 문서변환서버·hwplib 를
  대신할 `DocConverter` 구현체의 바탕이 되는 `rhwp.war` 와 `rhwp worker` 를 만든다.
- 구조 결정: [3단계 §4.1](stage3_reliability_and_server.md#41-결정-변경-2026-10-29) — 별도 WAR 컨텍스트 + 워커 프로세스 풀.
- 코드: [integrations/rhwp-was](../../integrations/rhwp-was/README.md)(WAR), `src/cli/commands/worker.rs`·`cell_text_actions.rs`(Rust)
- BMS 참조: `svnHSO_10.3_02_bms_trunk/src/main/java/com/hs/gwweb/converter/` (`DocConverter`, `HwpDocConverterService`,
  `action/InsertSignTextExAction`, `appr/sign/HwpSignActionUtil`)

## BMS 가 보내는 것

`HwpSignActionUtil.makeApprovalSignAction` 은 서명 셀 하나에 액션 두 개를 만든다.

| 순서 | 액션 | 값 | 글꼴 |
|---|---|---|---|
| 1 | `insertTextEx` (셀 내용 교체) | 결재자 이름 | `docconverter.hancom.signfont.*` (size 10, 굴림, 굵게) |
| 2 | `appendTextEx` (`appendLine=1`, 새 문단) | 날짜(`signDateFormat`) | datesize 7 |

날짜 위치 설정이 `top` 이면 순서가 바뀌고, 대결·전결이면 결재방식 글자가 앞에 붙는다. 서명 이미지면 `insertImage`·`appendPicture` 다(범위 밖).

## 만든 것

| 구성 | 내용 |
|---|---|
| `rhwp worker` | stdin 한 줄 JSON 요청 → stdout 한 줄 JSON 응답. 문서는 **경로**로 주고받는다. 요청마다 `catch_unwind` 격리. `{"ping":true}` 응답. 저장 후 다시 열어 필드 값 보고(`verify`) |
| 액션 | `insertTextEx`: `set_field_value_by_name_at` 로 셀 첫 문단 교체 후 글꼴 적용. `appendTextEx`: 마지막 셀 문단 끝에서 `split_paragraph_in_cell_native` → `insert_text_in_cell_native` → `apply_char_format_in_cell_native`. 값의 `\r` 은 문단 나눔. 글꼴은 `find_or_create_font_id_native`(이름)·pt×100·`0x00BBGGRR`→`#RRGGBB`·굵게 |
| `rhwp.war` | `POST /convert`(multipart `file`+`actions`) · `GET /health`. `WorkerPool`: 워커 K 개, 빈 워커 대기 상한(503), 요청 시간 상한(강제 종료 후 재기동), 밖에서 죽은 워커 교체, 컨텍스트 종료 시 전부 정리. Java 8 · javax.servlet 3.1 · org.json |

## 시험 결과 (Windows 개발 PC, Tomcat 8.5.90 별도 인스턴스, 통합서식 `tonghap_1page.hwp`)

| 항목 | 결과 |
|---|---|
| 워커 직접(NDJSON 5건) | ping 응답 · 서명.2 이름+날짜 2액션 성공(27ms) · 없는 필드 → `ok:false` · `서명.1` 에 `"10/29\r김철수"` → 문단 2개 · 깨진 JSON → `ok:false`, 프로세스 유지 |
| 저장본 구조 | 원본 대비 PARA_TEXT 컨트롤 순서 차이 0 (저장 결함 1·2 미발생 — 본문 누름틀 문단을 건드리지 않음) |
| 렌더링(SVG) | 서명.2 셀에 `홍길동`(10pt 굵게) 아래 `2026/10/29`(7pt, 시험용 빨강) 두 줄 표시 |
| HTTP 성공 | 200, 본문 14,848 bytes, `X-Rhwp-Result` 에 `verify.fieldValues["서명.2"]="홍길동"` |
| HTTP 오류 | 없는 필드 422 · 깨진 actions 400 · 파일 없음 400 |
| 워커 외부 강제 종료(2개 모두) | 다음 요청 200, `restarts=2`, 워커 2개 재기동. 요청 손실 없음 |
| 응답 시간 초과(`timeoutMs=1000`, 액션 300개) | 503 `응답 시간 초과 — 강제 종료`, `restarts=1`, 다음 정상 요청 200 |
| 동시 6건 / 워커 2개 | 전부 200(대기열 동작) |
| 컨텍스트 종료 | 워커 프로세스 0개 남음 |
| 임시 파일 | 요청 뒤 `rhwp-work/` 비어 있음 |
| Rust | `cargo clippy --bin rhwp -D warnings` 통과 |

**한글 확인(담당자, 2026-10-29):** `output/pr-review/bms-poc/` 의 4개 파일(원본, BMS 설정 서명, HTTP 경유 서명, `\r` 두 문단)을 한글에서 열어 비교했다.
오류·복구 안내 없이 열리고, 서명.2 셀에 `홍길동`(굵게) 아래 `2026/10/29`(작게)가 **별도 문단 두 줄**로 보인다. 서명.1 의 `"10/29\r김철수"` 도 두 문단이다.
결재 표·본문·누름틀 표시가 원본과 같고, 글자 수도 정확히 늘었다(157 → 170 = +13, 157 → 165 = +8).

`verify` 의 `fieldValues` 는 셀 필드의 첫 문단 값만 준다(날짜 줄은 포함되지 않음). 워커 요청은 UTF-8 이어야 한다(CP949 로 보내면 거절. `rhwp.war` 는 UTF-8 로 보낸다).

## 운영 투입 전 남은 것

| 항목 | 비고 |
|---|---|
| BMS `RhwpConverterService` | `CellAction` → actions JSON, multipart 호출, `X-Rhwp-Result` 해석, 실패 시 기존 변환기로 대체 경로 + 서킷 브레이커, `docconverter.class` 등록. 2단계 |
| `getFieldList`·`getFieldValues` 엔드포인트 | rhwp `fields` 로 작게 추가 |
| 그림(서명 이미지·관인) | `insertImage`·`appendPicture`. rhwp 그림 삽입의 파일 읽기·셀 삽입 보강 필요 |
| 리눅스 | rhwp 실행 파일 리눅스 빌드, 워커 메모리·처리 시간 실측, 운영 WAS 의 외부 프로세스 실행 허용 확인 |
| 보안 | `rhwp.token` 사용 또는 내부망 제한. 업로드 크기 상한은 100MB |
| 전용 WAS 배치 | 2대 이상 + BMS 쪽 URL 장애 조치 |
| `\r` 처리 차이 | 한컴 변환기는 문단 나눔으로 처리한다고 보고 같은 방식으로 했다. 한글 출력과 대조 필요 |
| `verify` 보강 | 셀 전체 문단 텍스트를 돌려주도록 |
| rhwp 호환 층 `CellShape.Width` | 서명 이미지 크기 계산 시 셀 폭 여부 확인(기안기 POC S5 와 공통) |
