# rhwp converter 2·3단계 구현 결과 (검증 보류)

(rhwp converter: rhwp 를 이용한 서버용 문서편집 개발. 계획은 [plan_rhwp_converter_stage2.md](plan_rhwp_converter_stage2.md))

- 일자: 2026-10-29
- 기준: 계획 커밋 `84ffdca16` 위에 구현. 담당자 지시로 **검증은 나중에 몰아서** 하기로 했다. 이 문서의 "확인" 은
  개발 PC(Windows)에서 워커를 직접 돌린 연기 시험이며, Tomcat HTTP 시험·리눅스 실측·한글 열기 확인·운영 서식 회귀는 미실행이다.
- 입력: `output/pr-review/bms-poc/0_original.hwp`(통합서식 `tonghap_1page`), 합성 PNG 120×60.

## 1. 구현 항목

| ID | 항목 | 구현 위치 | 상태 |
|---|---|---|---|
| W2 | 조회 API | 워커 `op:"fields"`·`op:"values"`, WAR `POST /fields`·`POST /field-values`(`FieldsServlet`) | 구현. 연기 시험 통과(57개 필드, `{{n}}` 순번, 셀 전체 글자) |
| W3 | `insertText`·빈 값·셀 비우기 | `cell_text_actions.rs`: `insertText`(글꼴 없음), 셀 필드 `insertTextEx`/`insertText` 는 셀 전체(문단·컨트롤·rhwp 형제 그림) 비운 뒤 넣기, 빈 값 허용. 코어 `delete_control_in_cell_native`(`object_ops/cell_controls.rs`) 추가 | 구현. 재서명·관인 비우기 연기 시험 통과. 셀 배경 그림은 비범위 |
| W6 | `verify` 보강 | `worker.rs`: 필드 목록 동일·본문 컨트롤 배치 동일(그림 액션의 형제 그림 증감만 허용)·쪽수 동일·셀 전체 문단 글자·셀 그림 수·누름틀 값 대조. 불일치면 `ok:false` + 출력 삭제(`strictVerify:false` 면 보고만) | 구현. 저장 결함 2 를 실제로 잡았다(아래 §2) |
| W4 | 그림 삽입 | `insertImage`(셀 비움 → 셀 안쪽 비율 맞춤 → `insert_picture_native` 셀 경로), `appendPicture`(새 문단 뒤). `imageFit`·`imageWidthMm/HeightMm`. WAR: `imagePart` multipart, URL 은 `rhwp.allowImageUrl` 일 때만 | 구현. 연기 시험 통과(형제 floating 그림 1개, 재비우기 시 제거). **크기·위치 기준(한컴 정답)은 미확정** |
| W1 | BMS `RhwpConverterService` | `integrations/rhwp-was/bms-client/.../RhwpConverterService.java`(원본) → BMS trunk `com.hs.gwweb.converter` 복사. `DocConvererServiceFactory` 분기, `globals_tpl.properties` 키 추가 | 구현. BMS 클래스패스로 `javac` 컴파일 통과. BMS 기동·결재 한 바퀴는 미실행. SVN 커밋 안 함 |
| W5 | 저장 결함 1·2 | `serializer/body_text.rs`: 본문 갭에서 이미 연 trailing 필드의 END 를 남은 컨트롤보다 먼저 닫음. `field_query.rs rebuild_char_offsets`: 첫 글자 시작 필드의 BEGIN 앞 컨트롤 수 하한. 단위 테스트 2개 추가 | 구현. 단위 테스트·실물 재확인은 §3 |
| W7 | 회귀 게이트 | `docs/fe-integration/scripts/converter_gate.mjs`: 서식 폴더 × 시나리오 12종(글자 서명 상/하, 대결, 두 셀, `\r`, 재서명, 이미지 상/하, 관인·문서번호 비우기, 결재일, 본문) → 워커 실행 → verify + `ctrlorder.py` → TSV/JSON | 구현. 운영 서식 미입수로 미실행 |
| W8 | 리눅스·운영 | `integrations/rhwp-was/linux/Dockerfile`·`build-linux.sh`(glibc/musl 선택), `/health` 에 waiting·busyRejects·lastError·workerVersion·apiVersion, 워커 프로토콜 버전 검사(apiVersion 2), `rhwp.token`·그림 상한, README 배치 절(전용 WAS 2대·URL 장애 조치) | 구현. 리눅스 빌드·실측 미실행(Docker 필요) |
| W9 | 한글 대조 | — | 미실행(정답 산출물 필요) |

## 2. 연기 시험 (워커 직접, `rhwp worker` NDJSON)

| 요청 | 결과 |
|---|---|
| `ping` | `apiVersion 2`, 0.8.7 |
| `fields option=2` | 57개(누름틀+셀). `option=0 number=1` 56개 셀 필드에 `{{1}}` |
| `values ["서명.2","본문","없음"]` | `""`, `"1"`(안내문 값), `null` |
| c1 서명.2 이름+날짜 | ok, verify ok, 셀 문단 `["홍길동","2026/10/29"]`, 16ms |
| c2 c1 결과에 재서명 | ok, 기존 2문단 병합·13자 제거 후 `["재서명"]` |
| c3 서명.1 `insertImage` + 날짜 | ok, 그림 1(5320×2660 HWPUNIT, 셀 안쪽 5320×2748 에 맞춤), 문단 `["","2026/10/29"]` |
| c4 c3 결과를 `insertTextEx("")` 로 비우기 | ok, 형제 그림 1 제거·10자 제거. (처음엔 verify 가 그림 감소를 변화로 잡아 규칙을 보정) |
| c5 `insertText(본문, "본문값테스트")` | **verify 실패**: 다시 열면 `본문` 값 `""` — 저장 결함 2 재현. 출력 삭제, `ok:false` |
| c6 서명.3 날짜 + `appendPicture 16×7mm` | ok, 그림 3969×1984, 문단 `["2026/10/29",""]` |
| c7 `insertImage` 에 imagePath 없음 | 거절(호스트가 그림을 준비해야 함) |

c5 는 W5 수정 **전** 바이너리의 결과다. W5 반영 후 재실행(같은 7건): **전부 ok, verify ok**, c5 의 `본문` 값이 다시 열어도
`본문값테스트` 로 남고 `ctrlorder.py` 차이 0. 단위 테스트 `serializer::body_text::tests`·`field_query::tests` 51건 통과(새 2건 포함),
`cargo clippy --bin rhwp -D warnings` 통과.

회귀 게이트 `converter_gate.mjs` 를 같은 서식 1건 × 시나리오 12종으로 실행: 12 PASS(verify ok, 컨트롤 순서 차이 0, 10~21ms).
산출물은 `output/pr-review/bms-stage2/`(Git 제외)에 있다 — 한글에서 열어 보는 확인은 아직이다.

## 3. 미검증·보류

| 항목 | 상태 |
|---|---|
| W5 실물 재확인: 통합서식 8종(`blank1/3`·`fixedrecv`·`3to4`·`5to6`·`audit`·`linked`) `formsweep`·`ctrlorder`, 한글 열기 | 미실행(`1page` 만 확인) |
| Clippy lib·WASM, `cargo fmt --check`(파일 단위) | 미실행(bin 만 통과) |
| Tomcat 에 WAR 올려 `/convert`(그림 파트 포함)·`/fields`·`/field-values`·`/health` HTTP 시험 | 미실행 |
| BMS 개발 서버에서 `docconverter.class=RhwpConverterService` 로 기안→결재→접수→반려, 워커 강제 종료·503 시 hwplib 대체, 브레이커 열림/닫힘 | 미실행 |
| 리눅스 빌드·워커 메모리/시간 실측·운영 WAS 외부 프로세스 허용 | 미실행 |
| 그림 크기·위치, 셀 비우기 결과, `\r` 문단 나눔, 날짜 글꼴을 한컴 변환기 산출물·한글 화면과 대조 | 정답 산출물 필요(계획 §5) |
| 운영 서식 전체 회귀 게이트 + 한글 열기 확인 | 운영 서식 필요 |
| 셀 배경 그림(HsCell `insertbackgroundpicture` 방식 관인) 제거 | 비범위. 필요하면 테두리/배경 채우기 편집 추가 |

## 4. 변경 이력

| 날짜 | 내용 |
|---|---|
| 2026-10-29 | W1~W8 구현, 워커 연기 시험, W5 단위 테스트·게이트 1건 실행. W9·전체 검증 보류 |
