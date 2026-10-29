# rhwp-was — rhwp converter 의 WAR 컨텍스트 (POC)

**rhwp converter**(rhwp 를 이용한 서버용 문서편집)의 WAS 쪽 구성 요소다.

BMS 서버에서 결재 본문 HWP 를 수정(서명 글자 넣기 등)하기 위한 WAR 다. BMS 와 같은 WAS 에
별도 컨텍스트(`/rhwp`)로 올리거나, 전용 WAS 에 올린다. 안에서 `rhwp worker` 프로세스 풀을
관리하고, 워커와는 **표준입출력 한 줄 JSON + 임시 파일 경로**로 통신한다.

```
BMS (RhwpConverterService) ──HTTP multipart──▶ /rhwp/convert
                                                  └─ WorkerPool: rhwp worker ×K (stdin/stdout NDJSON)
```

- 장애 격리: 워커가 죽거나 응답이 늦으면(`rhwp.timeoutMs`) 그 워커만 강제 종료하고 다시 띄운다.
- 대기열: 워커가 모두 바쁘면 `rhwp.queueTimeoutMs` 까지 기다린 뒤 503. BMS 는 대체 경로로 넘긴다.
- 컨텍스트를 내리면 워커를 전부 정리한다(고아 프로세스 없음).

## 빌드

```bash
mvn -DskipTests package          # target/rhwp.war (Java 8, javax.servlet 3.1)
```

## 설정

시스템 속성 > 환경 변수 > `WEB-INF/web.xml` context-param 순.

| 키 | 기본 | 설명 |
| --- | --- | --- |
| `rhwp.binary` / `RHWP_BINARY` | (필수) | `rhwp` 실행 파일 경로 |
| `rhwp.workers` | 2 | 워커 수. 서버 전체 워커 수가 코어 수를 넘지 않게 |
| `rhwp.timeoutMs` | 20000 | 요청 한 건 처리 상한. 넘기면 워커 강제 종료 |
| `rhwp.queueTimeoutMs` | 5000 | 빈 워커 대기 상한. 넘기면 503 |
| `rhwp.workDir` | 컨테이너 임시 폴더 | 입력·출력 임시 파일 위치 |
| `rhwp.token` | (없음) | 설정하면 `X-Rhwp-Token` 헤더 필수 |

## 로컬 시험

Tomcat 8.5/9 에 `target/rhwp.war` 를 올리고 JVM 옵션으로 설정을 준다. 기존 Tomcat 을 건드리지 않으려면
`CATALINA_BASE` 를 따로 만든다(`conf/` 복사 후 포트 변경, `webapps/rhwp.war`).

```bash
java -cp "$CATALINA_HOME/bin/bootstrap.jar;$CATALINA_HOME/bin/tomcat-juli.jar" \
  -Dcatalina.home="$CATALINA_HOME" -Dcatalina.base="$CATALINA_BASE" -Djava.io.tmpdir="$CATALINA_BASE/temp" \
  -Djava.util.logging.manager=org.apache.juli.ClassLoaderLogManager \
  -Drhwp.binary=/path/to/rhwp.exe -Drhwp.workers=2 -Drhwp.timeoutMs=15000 \
  org.apache.catalina.startup.Bootstrap start
# (pom 의 jetty-maven-plugin `mvn jetty:run` 은 플러그인 의존성이 받아져 있을 때만 동작한다)
curl -s http://localhost:8780/rhwp/health
curl -s -o out.hwp -D - http://localhost:8780/rhwp/convert \
  -F file=@tonghap_1page.hwp \
  -F 'actions=[{"action":"insertTextEx","fieldName":"서명.2","inputValue":"홍길동","fontName":"굴림","fontSize":"10","fontColor":"0x00000000","fontBold":"1"},
               {"action":"appendTextEx","fieldName":"서명.2","inputValue":"2026/10/29","fontName":"굴림","fontSize":"7","fontColor":"0x00000000","fontBold":"1","appendLine":"1"}]'
```

응답 200 의 본문이 수정된 HWP 이고, `X-Rhwp-Result` 헤더에 워커 결과 JSON(UTF-8 → Base64)이 실린다.
실패는 400(요청 오류) · 422(문서·액션 처리 실패, JSON) · 503(워커 없음·바쁨) · 401(토큰).

## API

### `POST /convert` (multipart/form-data)

| 파트 | 내용 |
| --- | --- |
| `file` | 원본 HWP/HWPX |
| `actions` | JSON 배열. 항목 키는 BMS `CellAction` 과 같다: `action`, `fieldName`, `inputValue`, `fontName`, `fontSize`(pt), `fontColor`(`0x00BBGGRR`), `fontBold`(`1`/`0`), `appendLine` |
| `verify` | `false` 면 저장 후 재확인 생략 |

지원 액션(1단계): `insertTextEx`(셀 필드 내용 교체), `appendTextEx`(셀에 문단 추가). 값의 `\r` 은 문단 나눔.

### `GET /health`

풀 상태 JSON. 워커가 하나도 없으면 503.

## 워커 프로토콜 (`rhwp worker`)

요청 한 줄 → 응답 한 줄. 문서는 경로로 주고받는다.

```json
{"id":"…","input":"/tmp/a_in.hwp","output":"/tmp/a_out.hwp","verify":true,
 "actions":[{"action":"insertTextEx","fieldName":"서명.2","inputValue":"홍길동","fontSize":"10"}]}
{"id":"…","ok":true,"output":"/tmp/a_out.hwp","outputBytes":14336,"actions":[…],
 "verify":{"reloaded":true,"pageCount":1,"fieldValues":{"서명.2":"홍길동"}},"elapsedMs":31}
```

`{"ping":true}` → `{"pong":true,"version":"…"}`. 요청 하나의 panic 은 `ok:false` 로 격리된다.

## 범위 밖 (후속)

- `getFieldList`·`getFieldValues` 엔드포인트, 그림(서명 이미지·관인) 삽입, PDF
- BMS `RhwpConverterService`(대체 경로·서킷 브레이커 포함) — 2단계
- 리눅스용 rhwp 실행 파일 빌드·메모리 실측, 운영 WAS 외부 프로세스 실행 정책 확인
