# 검증 스크립트

1~3단계 검토에 쓴 스크립트다. `<pkg>`는 저장소 루트에서 `scripts/wasm-pack-locked.*`로 만든 web 패키지
디렉터리(`pkg/`)이고, Node는 `rhwp.js`·`rhwp_bg.wasm`을 `initSync`로 적재한다. 입력 서식은 커밋하지 않았다.

| 스크립트 | 용도 | 사용법 |
|---|---|---|
| `picture_test.mjs` | 필드명 → 셀 경로 → 셀 크기에 맞춘 그림 삽입 → 저장 → 다시 열기 | `node picture_test.mjs <pkg> <작업 디렉터리>` |
| `roundtrip.mjs` | 편집 없이 열기 → `exportHwp` 왕복 | `node roundtrip.mjs <pkg> <입력> <출력>` |
| `formsweep.mjs` | 폴더의 모든 서식에 (a) 모든 필드 글자 채우기 (b) 모든 셀 필드 그림 → 저장 → 다시 열어 값 비교 | `node formsweep.mjs <pkg> <입력 폴더> <출력 폴더> <stamp.png>` |
| `bench.mjs` | 호출 비용 측정: 요청마다 CLI 실행 / CLI 기동만 / 상주 WASM 단계별 | `node bench.mjs <rhwp.exe> <pkg> <입력.hwp> <fill.json> <stamp.png> [N]` |
| `fill.json` | `bench.mjs`용 통합서식 필드 16개 시험값 | |
| `paratext.py` | 최소 CFB 리더. 첫 최상위 문단의 PARA_HEADER·PARA_TEXT 토큰 출력 | `python -I paratext.py <파일.hwp>` |
| `paratext_lib.py` | `paratext.py`의 함수 모음(`ctrlorder.py`가 사용) | |
| `ctrlorder.py` | 두 HWP의 모든 PARA_TEXT 컨트롤 순서 비교(FIELD_END 정규화) | `python -I ctrlorder.py <원본.hwp> <저장본.hwp> [--ignore-gso]` |
| `converter_gate.mjs` | rhwp converter 회귀 게이트: 서식 폴더 × BMS 액션 시나리오를 `rhwp worker` 로 실행 → verify + `ctrlorder.py` → TSV/JSON | `node converter_gate.mjs <rhwp> <입력 폴더> <출력 폴더> [--image sign.png] [--scenarios f.json]` |

외부 Python 패키지는 필요 없다.
