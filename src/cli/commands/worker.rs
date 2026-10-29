//! `rhwp worker` — 상주 워커 모드 (BMS 문서변환용, POC).
//!
//! WAS(`rhwp.war`)가 이 프로세스를 띄워 두고 stdin 으로 한 줄 JSON 요청을 보내면, 한 줄
//! JSON 응답을 stdout 으로 돌려준다. 문서는 **파일 경로**로 주고받는다 — 같은 서버이므로
//! base64 로 실을 이유가 없다. 요청마다 `catch_unwind` 로 격리하므로 한 요청의 panic 이
//! 프로세스를 죽이지 않는다. 프로세스가 죽거나 응답이 늦으면 WAS 가 강제 종료 후 다시 띄운다.
//!
//! 요청: `{"id":…, "input":"<경로>", "output":"<경로>", "actions":[{…}], "verify":true}`
//! 응답: `{"id":…, "ok":true, "output":…, "actions":[…], "verify":{…}, "elapsedMs":…}`
//!       `{"id":…, "ok":false, "error":"…"}`
//! `{"ping":true}` 에는 `{"pong":true,"version":…}` 로 답한다. stdin 이 닫히면 끝난다.
//!
//! 액션은 [`super::cell_text_actions`] 가 처리한다(`insertTextEx`·`appendTextEx`).

use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};

use super::cell_text_actions::{self, CellTextAction};
use super::edit::runtime::{edit_output_format, edit_serialize};
use crate::cli::document_io::{load_document, LoadError};

const EXIT_OK: i32 = 0;
const EXIT_RUNTIME: i32 = 1;

pub(crate) fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("사용법: rhwp worker  (stdin 한 줄 JSON 요청 → stdout 한 줄 JSON 응답, EOF 까지)");
        return EXIT_OK;
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("worker: stdin 읽기 실패 - {e}");
                return EXIT_RUNTIME;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&line);
        if writeln!(out, "{response}")
            .and_then(|_| out.flush())
            .is_err()
        {
            eprintln!("worker: stdout 쓰기 실패 (호스트가 닫힘)");
            return EXIT_RUNTIME;
        }
    }
    EXIT_OK
}

/// 한 줄을 처리해 응답 한 줄을 만든다. panic 은 여기서 잡혀 `ok:false` 가 된다.
fn handle_line(line: &str) -> Value {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "error": format!("요청 JSON 파싱 실패: {e}") }),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("ping").and_then(Value::as_bool) == Some(true) {
        return json!({ "id": id, "pong": true, "version": env!("CARGO_PKG_VERSION") });
    }
    let started = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| process(&request)));
    let mut response = match result {
        Ok(Ok(mut v)) => {
            v["ok"] = json!(true);
            v
        }
        Ok(Err(e)) => json!({ "ok": false, "error": e }),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "알 수 없는 panic".into());
            json!({ "ok": false, "error": format!("panic: {msg}") })
        }
    };
    response["id"] = id;
    response["elapsedMs"] = json!(started.elapsed().as_millis() as u64);
    response
}

fn str_field<'a>(request: &'a Value, key: &str) -> Result<&'a str, String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("'{key}' 가 필요합니다"))
}

fn process(request: &Value) -> Result<Value, String> {
    let input = str_field(request, "input")?;
    let output = str_field(request, "output")?;
    let verify = request
        .get("verify")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let actions: Vec<CellTextAction> = request
        .get("actions")
        .and_then(Value::as_array)
        .ok_or("'actions' 배열이 필요합니다")?
        .iter()
        .map(CellTextAction::from_json)
        .collect::<Result<_, _>>()?;
    if actions.is_empty() {
        return Err("actions 가 비었습니다".into());
    }

    let bytes =
        fs::read(input).map_err(|e| format!("입력 파일을 읽을 수 없습니다 - {input}: {e}"))?;
    let mut doc = load_document(&bytes).map_err(|e| match e {
        LoadError::NeedPassword => "암호 문서입니다 (비밀번호 없음)".to_string(),
        LoadError::WrongPassword => "비밀번호 불일치".to_string(),
        LoadError::Other(msg) => format!("문서를 열 수 없습니다: {msg}"),
    })?;

    let mut results = Vec::with_capacity(actions.len());
    for action in &actions {
        // 하나라도 실패하면 출력 파일을 쓰지 않는다 (edit 명령군과 같은 "실패 시 원본 불변").
        let r = cell_text_actions::apply(&mut doc, action)
            .map_err(|e| format!("{}({}): {e}", action.action, action.field_name))?;
        results.push(r);
    }

    let format = edit_output_format(&bytes, Some(output));
    let out_bytes = edit_serialize(&mut doc, format).map_err(|e| format!("직렬화 실패: {e}"))?;
    write_atomic(Path::new(output), &out_bytes)?;

    let mut response = json!({
        "output": output,
        "outputBytes": out_bytes.len(),
        "actions": results,
    });
    if verify {
        response["verify"] = verify_output(&out_bytes, &actions);
    }
    Ok(response)
}

/// 같은 폴더에 임시 파일로 쓴 뒤 이름을 바꾼다. 반쯤 쓰인 파일이 결과로 보이지 않게.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!(
        "{}.rhwp-tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("hwp")
    ));
    fs::write(&tmp, bytes)
        .map_err(|e| format!("출력 파일을 쓸 수 없습니다 - {}: {e}", tmp.display()))?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("출력 파일 이름 변경 실패 - {}: {e}", path.display())
    })
}

/// 저장한 바이트를 다시 열어 액션 대상 필드의 값을 읽는다. 호스트가 기대값과 대조한다.
fn verify_output(out_bytes: &[u8], actions: &[CellTextAction]) -> Value {
    let doc = match load_document(out_bytes) {
        Ok(d) => d,
        Err(_) => return json!({ "reloaded": false }),
    };
    let fields = doc.collect_all_fields();
    let mut values = serde_json::Map::new();
    for action in actions {
        if values.contains_key(&action.field_name) {
            continue;
        }
        let value = fields
            .iter()
            .find(|f| f.field.field_name() == Some(action.field_name.as_str()))
            .map(|f| f.value.clone());
        values.insert(
            action.field_name.clone(),
            value.map(Value::String).unwrap_or(Value::Null),
        );
    }
    json!({ "reloaded": true, "pageCount": doc.page_count(), "fieldValues": values })
}
