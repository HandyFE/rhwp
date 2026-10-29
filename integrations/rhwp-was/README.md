# rhwp-was — rhwp converter 의 WAR 컨텍스트

**rhwp converter**(rhwp 를 이용한 서버용 문서편집)의 WAS 쪽 구성 요소다.

BMS 서버에서 결재 본문 HWP 를 수정(서명 글자·이미지, 관인 제거, 결재일 등)하기 위한 WAR 다. BMS 와 같은
WAS 에 별도 컨텍스트(`/rhwp`)로 올리거나, 전용 WAS 에 올린다. 안에서 `rhwp worker` 프로세스 풀을
관리하고, 워커와는 **표준입출력 한 줄 JSON + 임시 파일 경로**로 통신한다.

```
BMS (RhwpConverterService) ──HTTP multipart──▶ /rhwp/convert · /fields · /field-values
                                                  └─ WorkerPool: rhwp worker ×K (stdin/stdout NDJSON)
```

- 장애 격리: 워커가 죽거나 응답이 늦으면(`rhwp.timeoutMs`) 그 워커만 강제 종료하고 다시 띄운다.
- 대기열: 워커가 모두 바쁘면 `rhwp.queueTimeoutMs` 까지 기다린 뒤 503. BMS 는 대체 경로로 넘긴다.
- 저장 후 재확인: 워커가 저장본을 다시 열어 필드 목록·본문 컨트롤 배치·쪽수·셀 글자·그림 수를 기대값과
  대조한다. 하나라도 다르면 422 로 거절하고 출력을 지운다(무증상 저장 손상을 운영에서 막는 장치).
- 컨텍스트를 내리면 워커를 전부 정리한다(고아 프로세스 없음).
- 기동 시 워커의 프로토콜 버전(`apiVersion`)을 확인한다. WAR 와 rhwp 실행 파일의 짝이 맞지 않으면 풀이
  뜨지 않고 `/health` 가 원인을 알려 준다.

## 구성 요소

| 위치 | 내용 |
| --- | --- |
| `src/main/java/com/handysoft/rhwp/was/` | WAR (Java 8, javax.servlet 3.1, org.json) |
| `bms-client/.../RhwpConverterService.java` | BMS 쪽 `DocConverter` 구현체의 원본. BMS SVN `com.hs.gwweb.converter` 에 복사해 쓴다 |
| `linux/` | 리눅스용 rhwp 실행 파일 Docker 빌드 |
| rhwp-FE `src/cli/commands/worker.rs`·`cell_text_actions.rs` | 워커(Rust) |

## 빌드

```bash
mvn -DskipTests package          # target/rhwp.war (Java 8, javax.servlet 3.1)
```

rhwp 실행 파일: Windows 는 `cargo build --release --bin rhwp --target-dir target/pr-review`,
리눅스는 [linux/build-linux.sh](linux/build-linux.sh)(Docker). 서버 glibc 가 낮으면 베이스를 낮추거나 musl 정적 빌드를 쓴다.

## 설정

시스템 속성 > 환경 변수 > `WEB-INF/web.xml` context-param 순.

| 키 | 기본 | 설명 |
| --- | --- | --- |
| `rhwp.binary` / `RHWP_BINARY` | (필수) | `rhwp` 실행 파일 경로 |
| `rhwp.workers` | 2 | 워커 수. 서버 전체 워커 수가 코어 수를 넘지 않게. 워커 하나는 문서 1건 처리에 수십 ms, 상주 메모리 수십 MB |
| `rhwp.timeoutMs` | 20000 | 요청 한 건 처리 상한. 넘기면 워커 강제 종료 |
| `rhwp.queueTimeoutMs` | 5000 | 빈 워커 대기 상한. 넘기면 503 |
| `rhwp.workDir` | 컨테이너 임시 폴더 | 입력·출력·그림 임시 파일 위치 |
| `rhwp.token` | (없음) | 설정하면 모든 요청에 `X-Rhwp-Token` 헤더 필수. 내부망이라도 전용 WAS 로 둘 때는 설정한다 |
| `rhwp.allowImageUrl` | false | true 면 그림 액션의 http(s) URL `inputValue` 를 WAR 가 내려받는다. 기본은 BMS 가 multipart 로 첨부 |
| `rhwp.maxImageBytes` | 5242880 | 그림 한 장 상한 |
| `-Drhwp.keepWork=true` | | 디버깅용. 작업 파일을 지우지 않는다 |

## API

모든 요청은 `multipart/form-data`. 실패는 400(요청 오류) · 422(문서·액션 처리·재확인 실패, JSON) · 503(워커 없음·바쁨) · 401(토큰).

### `POST /convert`

| 파트 | 내용 |
| --- | --- |
| `file` | 원본 HWP/HWPX |
| `actions` | JSON 배열. 항목 키는 BMS `CellAction` 과 같다: `action`, `fieldName`, `inputValue`, `fontName`, `fontSize`(pt), `fontColor`(`0x00BBGGRR`), `fontBold`(`1`/`0`), `appendLine`. 그림 액션은 `imagePart`(파트 이름) 추가 |
| `img<n>` 등 | 그림 액션이 가리키는 그림 파일 파트 |
| `verify` | `false` 면 저장 후 재확인 생략 |
| `strictVerify` | `false` 면 재확인 불일치를 오류로 보지 않고 결과만 헤더에 싣는다 |

응답 200 의 본문이 수정된 HWP 이고, `X-Rhwp-Result` 헤더에 워커 결과 JSON(UTF-8 → Base64)이 실린다.

지원 액션 (BMS `converter/action` 16종 중 `addPageCell`·조회 2종 제외):

| 액션 | 의미 |
| --- | --- |
| `insertTextEx`, `insertSignTextEx`, `insertSignDateTextEx` | 셀 필드: 셀 내용 전체(글자·문단·그림)를 지우고 글자를 넣는다(글꼴 적용). 누름틀: 값만. 빈 값은 비우기 |
| `appendTextEx`, `appendSignTextEx`, `appendSignDateTextEx` | 셀 끝에 문단 추가 + 글자(글꼴 적용) |
| `insertText` | `insertTextEx` 와 같되 글꼴을 바꾸지 않는다 |
| `insertImage` | 셀을 비우고 그림을 셀 안쪽 크기에 비율 유지로 맞춰 넣는다 |
| `appendPicture` | 셀 끝에 문단 추가 + 그림 |

값의 `\r` 은 문단 나눔. 그림 크기는 `imageFit`(0~1, 기본 1) 또는 `imageWidthMm`+`imageHeightMm`(상자) 로 조절한다.
셀 배경 그림(테두리/배경 채우기)은 비우기 대상이 아니다.

### `POST /fields`

`file`, `option`(0 셀 필드·1 누름틀·2 전부, 기본 0), `number`(0 없음·1 순번 `{{n}}`·2 개수) → `{"ok":true,"names":[…],"fields":[{name,cellField,section,para,depth}],"count":n}`.
BMS `getFieldList` 는 hwplib 구현체와 같이 `option=2` 를 쓴다.

### `POST /field-values`

`file`, `fields`(JSON 배열 또는 쉼표 구분) → `{"ok":true,"values":{"miskey":"…"}}`. 셀 필드는 셀 전체 글자(문단은 `\n`), 없으면 `null`.

### `GET /health`

풀 상태 JSON(`workers.configured/pooled/alive(살아 있는 프로세스 수)/idle/waiting/restarts/failures/busyRejects/requests(누적)/workerVersion/lastError`, `apiVersion`, `workerApiVersion`, `endpoints`). 워커가 하나도 없으면 503.

## 워커 프로토콜 (`rhwp worker`, apiVersion 2)

요청 한 줄 → 응답 한 줄. 문서는 경로로 주고받는다.

```json
{"id":"…","op":"convert","input":"/tmp/a_in.hwp","output":"/tmp/a_out.hwp","verify":true,
 "actions":[{"action":"insertTextEx","fieldName":"서명.2","inputValue":"홍길동","fontSize":"10"},
            {"action":"insertImage","fieldName":"서명.3","imagePath":"/tmp/a_img1.bin"}]}
{"id":"…","ok":true,"output":"/tmp/a_out.hwp","outputBytes":14336,"actions":[…],
 "verify":{"ok":true,"reloaded":true,"pageCount":1,"pageCountBefore":1,"fieldValues":{…},"cellTexts":{…},"pictures":{…},"mismatches":[]},"elapsedMs":31}
{"op":"fields","input":"/tmp/a_in.hwp","option":2}
{"op":"values","input":"/tmp/a_in.hwp","fields":["miskey"]}
{"ping":true}  →  {"pong":true,"version":"…","apiVersion":2}
```

요청 하나의 panic 은 `ok:false` 로 격리된다. 재확인 실패는 `ok:false` + 출력 삭제(`strictVerify:false` 면 결과만 보고).

## BMS 연동 (`RhwpConverterService`)

1. `bms-client/src/main/java/com/hs/gwweb/converter/RhwpConverterService.java` 를 BMS `com.hs.gwweb.converter` 에 둔다.
2. `DocConvererServiceFactory` 에 `@Qualifier("RhwpConverterService")` 주입과 `indexOf("Rhwp")` 분기를 추가한다.
3. `globals.properties`:
   ```
   docconverter.use=true
   docconverter.class=RhwpConverterService
   docconverter.rhwp.url=http://127.0.0.1:8080/rhwp
   docconverter.rhwp.token=
   docconverter.rhwp.fallback.class=HwpLibConverterService
   docconverter.rhwp.breaker.failures=3
   docconverter.rhwp.breaker.openms=300000
   docconverter.rhwp.image.timeoutms=5000
   docconverter.rhwp.verify=true
   ```
   HTTP 시간 상한은 기존 `docconverter.http.readtimeoutms`·`connectiontimeoutms` 를 쓴다.
4. 동작: rhwp 실패(연결·503·시간 초과·422)는 대체 구현체로 넘기고, 연속 N 회 실패면 브레이커를 열어 `openms` 동안
   rhwp 를 부르지 않는다(이후 1건 시험 후 복귀). `addPageCell` 처럼 rhwp 가 지원하지 않는 액션은 실패로 세지 않고
   대체 구현체로 넘긴다. 서명 이미지는 BMS 가 URL 에서 내려받아 multipart 로 첨부한다.

## 배치

- **같은 WAS**: `webapps/rhwp.war` + JVM 옵션 `-Drhwp.binary=…`. BMS 는 `http://127.0.0.1:<port>/rhwp`.
- **전용 WAS**: 여러 BMS 가 함께 쓰므로 공용 장애 지점이다. 2대 이상 두고 BMS 쪽 `docconverter.rhwp.url` 을
  L4/프록시 VIP 로 가리키거나, 장애 시 설정을 바꿔 다른 노드로 돌린다. `rhwp.token` 을 설정한다.
- 운영 WAS 가 외부 프로세스 실행(`ProcessBuilder`)을 막으면(보안 관리자 등) 전용 WAS 로 둔다.
- 워커 메모리·처리 시간은 리눅스 서버에서 실측해 `rhwp.workers`·`timeoutMs` 를 정한다. 개발 PC(Windows) 실측은
  서명 2액션 27ms, 임시 파일 왕복 포함 HTTP 200 수십 ms 였다.

## 로컬 시험

Tomcat 8.5/9 에 `target/rhwp.war` 를 올리고 JVM 옵션으로 설정을 준다. 기존 Tomcat 을 건드리지 않으려면
`CATALINA_BASE` 를 따로 만든다(`conf/` 복사 후 포트 변경, `webapps/rhwp.war`).
개발 PC 에서 쓴 인스턴스 구성(복사 절차·`setenv.bat`·포트 8480)은 `local-tomcat/README.md` 에 있다.

```bash
java -cp "$CATALINA_HOME/bin/bootstrap.jar;$CATALINA_HOME/bin/tomcat-juli.jar" \
  -Dcatalina.home="$CATALINA_HOME" -Dcatalina.base="$CATALINA_BASE" -Djava.io.tmpdir="$CATALINA_BASE/temp" \
  -Djava.util.logging.manager=org.apache.juli.ClassLoaderLogManager \
  -Drhwp.binary=/path/to/rhwp.exe -Drhwp.workers=2 -Drhwp.timeoutMs=15000 \
  org.apache.catalina.startup.Bootstrap start
curl -s http://localhost:8780/rhwp/health
curl -s -o out.hwp -D - http://localhost:8780/rhwp/convert \
  -F file=@tonghap_1page.hwp \
  -F 'actions=[{"action":"insertTextEx","fieldName":"서명.2","inputValue":"홍길동","fontName":"굴림","fontSize":"10","fontColor":"0x00000000","fontBold":"1"},
               {"action":"appendTextEx","fieldName":"서명.2","inputValue":"2026/10/29","fontName":"굴림","fontSize":"7","fontColor":"0x00000000","fontBold":"1","appendLine":"1"},
               {"action":"insertImage","fieldName":"서명.3","imagePart":"img0"}]' \
  -F img0=@sign.png
curl -s http://localhost:8780/rhwp/fields -F file=@tonghap_1page.hwp -F option=2
curl -s http://localhost:8780/rhwp/field-values -F file=@tonghap_1page.hwp -F 'fields=["miskey","서명.2"]'
```

Windows Git Bash 에서는 한글이 든 `actions`·`fields` 를 UTF-8 파일로 두고 `-F 'actions=<a.json;type=application/json;charset=utf-8'` 로 보낸다(인자로 직접 주면 cp949 로 전송된다).

회귀 게이트(운영 서식 전체 × 시나리오)는 `docs/fe-integration/scripts/converter_gate.mjs` 로 워커를 직접 돌린다.

## 범위 밖

- `addPageCell`(을지 병합) — 호출처 없음, 사용 여부 확인 뒤 결정
- PDF 출력, 셀 배경 그림 제거
