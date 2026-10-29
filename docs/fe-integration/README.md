# rhwp-FE: 핸디 기안기·BMS 적용 검토

rhwp-FE는 원작 rhwp(v0.8.7, `1a76570e8`)를 바탕으로 **핸디 전자결재 기안기(FeWebClient)와 BMS 서버**에 쓰기 위해
추가 개발하는 회사 포크다. 이 폴더는 적용 검토 결과, 결정 사항과 검증 스크립트를 모은다.

## 운영 원칙

- **원작 저장소(edwardkim/rhwp)에 PR을 내지 않는다**(당분간). 결함 수정과 기능 개선은 이 포크에서 한다.
- 저장소 루트의 `CLAUDE.md`·`AGENTS.md`·`CONTRIBUTING.md`·`mydocs/`는 원작 프로젝트의 지침이다.
  회사 작업 방식과 충돌하면 회사 방식을 우선하되, 판단이 필요하면 담당자에게 확인한다.
- 큰 작업은 착수 전에 범위·견적을 보고하고 확인받는다.
- **worktree를 쓰지 않는다.** `C:\Handyground\Workspace\git\rhwp-FE`의 `main` 브랜치에서 직접 작업한다(2026-10-29).
  Claude Code 세션은 worktree 없이 시작하고, 파일 경로는 원래 체크아웃 기준으로 안내한다.
- 커밋은 담당자가 지시할 때만 한다. `cargo fmt --all`은 저장소 전체 줄바꿈을 바꾸므로 쓰지 않는다(필요하면 파일 단위 `rustfmt`).
- 회사 문서는 원작 문서(`mydocs/`)와 섞지 않고 이 `docs/` 아래에 둔다.

## 작업 명칭

| 명칭 | 내용 | 코드 |
|---|---|---|
| **rhwp editor** | rhwp 를 이용한 기안기(FeWebClient)용 에디터 개발 | FeWebClient_rhwp `src/editor/rhwp/`, rhwp-FE `rhwp-studio`·`npm/hwpctrl-ocx` |
| **rhwp converter** | rhwp 를 이용한 서버(BMS)용 문서편집 개발 | rhwp-FE `integrations/rhwp-was/`(WAR·`bms-client/`·`linux/`), `src/cli/commands/worker.rs`·`cell_text_actions.rs`, `docs/fe-integration/scripts/converter_gate.mjs` |

## 문서

| 문서 | 내용 |
|---|---|
| [poc_interim_report.md](poc_interim_report.md) | **두 POC(rhwp editor·rhwp converter) 중간 결과 보고** — rhwp 설명, 결과, 제품 포함 판정, 다음 단계(2026-10-29). 최종 보고·발표 자료의 바탕 |
| [stage1_verification.md](stage1_verification.md) | 1단계: FE 서식을 rhwp로 열기·필드 채우기·셀 그림 삽입·저장 검증, 한글 확인 결과와 저장 결함 1 원인 |
| [stage2_api_scope.md](stage2_api_scope.md) | 2단계: 기안기(웹한글 `hwpCtrl`) 대체에 필요한 API 우선순위와 견적(61~91인일) |
| [stage3_reliability_and_server.md](stage3_reliability_and_server.md) | 3단계: 제품 적용 신뢰성 평가, 저장 결함 2 발견, BMS 서버 운영 방식 비교와 결정 |
| [poc_rhwp_editor.md](poc_rhwp_editor.md) | **rhwp editor** POC: 기안기용 에디터(웹한글 대체). 구조, 빌드·켜기, 확인 결과 |
| [poc_rhwp_converter.md](poc_rhwp_converter.md) | **rhwp converter** POC 1단계: 서버용 문서편집(BMS). `rhwp worker` + `rhwp.war` 로 `서명.2` 글자 서명, 장애 시험 결과 |
| [plan_rhwp_converter_stage2.md](plan_rhwp_converter_stage2.md) | **rhwp converter** 2·3단계 계획: BMS `DocConverter` 계약 대비 공백, 작업 항목 W1~W9 와 견적, 착수 전 요청 자료 |
| [rhwp_converter_stage2.md](rhwp_converter_stage2.md) | **rhwp converter** 2·3단계 구현 결과: W1~W8 구현, 워커 연기 시험, 미검증 목록 |
| [scripts/](scripts/README.md) | 검증에 쓴 스크립트 |

## 현재 결론 (2026-10-28)

| 용도 | 판정 |
|---|---|
| 문서 읽기·렌더링 | 제품에 쓸 만함 |
| 서버에서 편집 후 HWP 저장(필드·서명·관인) | 현재 그대로는 미달. 통합서식 8종 중 7종에서 저장 결함 재현 |
| 기안기 편집기 대체 | 시기상조 |

알려진 결함(2026-10-29 rhwp converter 2단계에서 수정, 실물 재검증 보류 — [결과](rhwp_converter_stage2.md) §3):

| ID | 증상 | 원인 위치 | 영향 서식 |
|---|---|---|---|
| 결함 1 | 저장 시 `본문` 누름틀 끝 표시가 뒤 개체(결재 표 등) 뒤로 이동. 결재 표가 누름틀 안에 들어가거나 한글에서 표·그림이 사라짐 | `src/serializer/body_text.rs` `serialize_para_text()` 남은 컨트롤 루프의 고아 FIELD_END 경로 | 통합서식 다수 |
| 결함 2 | 문단이 표로 시작하고 바로 뒤에 `본문` 누름틀이 오면, 채운 값이 문단 맨 앞(secd 뒤)에 저장되어 다시 열면 필드가 빔 | 같은 함수의 위치 계산 | `1page`·`audit`·`3to4`·`5to6` |

## 결정 기록

| 일자 | 결정 | 근거 |
|---|---|---|
| 2026-10-28 | 원작 저장소에 PR을 내지 않고 이 포크에서 개발한다(당분간) | 핸디 기안기 전용 추가 개발 저장소 |
| 2026-10-28 | BMS 연동은 **Rust 상주 HTTP 서비스**로 한다. 구현은 나중에 착수 | [3단계 §3·§4](stage3_reliability_and_server.md#4-결정-기록) |
| 2026-10-29 | (변경) rhwp converter 는 **별도 WAR 컨텍스트 `rhwp.war` + 워커 프로세스 풀**로 한다. 1단계 `서명.2` 글자 서명 착수 | [3단계 §4.1](stage3_reliability_and_server.md#41-결정-변경-2026-10-29), [integrations/rhwp-was](../../integrations/rhwp-was/README.md) |
| 2026-10-28 | 결함 수정·기능 개선은 기안기·BMS 적용 작업 때 진행한다 | 현 단계는 신뢰성 판단이 목적 |

## 다음 단계 (보류 중)

1. 착수 전 확인: BMS 처리량, 서버 OS·배포 방식, 실패 시 대체 경로, 운영 서식 목록
2. 저장 결함 1·2 수정
3. 버전 고정 + 운영 서식 전체 회귀 게이트(`scripts/formsweep.mjs` 정식화, 한글 열기 확인 포함)
4. rhwp converter 2·3단계([계획](plan_rhwp_converter_stage2.md)) — W1~W8 구현 완료([결과](rhwp_converter_stage2.md)), 검증(Tomcat·BMS·리눅스·한글 대조·회귀 게이트) 보류
5. 기안기 대체는 2단계 견적을 바탕으로 범위 조정 후 결정

## 증적

검증 산출물(HWP, JSON, 덤프)은 Git 제외 경로 `output/pr-review/fe-stage1/`에 있다.
입력 서식(FE·통합서식)은 사내 자료라 커밋하지 않았다.
