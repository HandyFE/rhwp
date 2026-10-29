//! `rhwp worker` — 상주 워커 모드 (rhwp converter, BMS 문서변환용).
//!
//! WAS(`rhwp.war`)가 이 프로세스를 띄워 두고 stdin 으로 한 줄 JSON 요청을 보내면, 한 줄
//! JSON 응답을 stdout 으로 돌려준다. 문서는 **파일 경로**로 주고받는다 — 같은 서버이므로
//! base64 로 실을 이유가 없다. 요청마다 `catch_unwind` 로 격리하므로 한 요청의 panic 이
//! 프로세스를 죽이지 않는다. 프로세스가 죽거나 응답이 늦으면 WAS 가 강제 종료 후 다시 띄운다.
//!
//! 요청 종류(`op`, 기본 `convert`):
//! - `convert`: `{"id":…, "input":"<경로>", "output":"<경로>", "actions":[{…}], "verify":true}`
//!   → `{"id":…, "ok":true, "output":…, "actions":[…], "verify":{…}, "elapsedMs":…}`.
//!   `verify` 가 실패하면 `ok:false` 이고 출력 파일을 지운다(호스트는 대체 경로로 넘긴다).
//! - `fields`: `{"op":"fields","input":"<경로>","option":0,"number":0}` → 필드 이름 목록.
//!   `option` 0 셀 필드, 1 누름틀, 2 전부(HsCell `getfieldlist` 과 같다). `number` 1 이면
//!   이름 뒤에 `{{n}}` 순번, 2 이면 같은 이름 개수.
//! - `values`: `{"op":"values","input":"<경로>","fields":["miskey"]}` → 이름별 값(없으면 null).
//!   셀 필드는 셀 전체 글자(문단은 `\n`).
//! - `{"ping":true}` → `{"pong":true,"version":…,"apiVersion":2}`. stdin 이 닫히면 끝난다.
//!
//! 액션은 [`super::cell_text_actions`] 가 처리한다.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

use super::cell_text_actions::{self, ActionKind, CellTextAction};
use super::edit::runtime::{edit_output_format, edit_serialize};
use crate::cli::document_io::{load_document, LoadError};

const EXIT_OK: i32 = 0;
const EXIT_RUNTIME: i32 = 1;
/// 호스트가 기동 시 확인하는 워커 프로토콜 버전. 요청·응답 모양이 바뀌면 올린다.
pub(crate) const API_VERSION: u32 = 2;

pub(crate) fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("사용법: rhwp worker  (stdin 한 줄 JSON 요청 → stdout 한 줄 JSON 응답, EOF 까지)");
        println!("  op: convert(기본) | fields | values, 또는 {{\"ping\":true}}");
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
        return json!({ "id": id, "pong": true, "version": env!("CARGO_PKG_VERSION"), "apiVersion": API_VERSION });
    }
    let started = Instant::now();
    let op = request
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or("convert")
        .to_string();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op.as_str() {
        "convert" => process(&request),
        "fields" => list_fields(&request),
        "values" => read_values(&request),
        other => Err(format!("알 수 없는 op: {other} (convert·fields·values)")),
    }));
    let mut response = match result {
        Ok(Ok(mut v)) => {
            if v.get("ok").is_none() {
                v["ok"] = json!(true);
            }
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
    response["op"] = json!(op);
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

fn load_input(request: &Value) -> Result<(Vec<u8>, HwpDocument), String> {
    let input = str_field(request, "input")?;
    let bytes =
        fs::read(input).map_err(|e| format!("입력 파일을 읽을 수 없습니다 - {input}: {e}"))?;
    let doc = load_document(&bytes).map_err(|e| match e {
        LoadError::NeedPassword => "암호 문서입니다 (비밀번호 없음)".to_string(),
        LoadError::WrongPassword => "비밀번호 불일치".to_string(),
        LoadError::Other(msg) => format!("문서를 열 수 없습니다: {msg}"),
    })?;
    Ok((bytes, doc))
}

// ---------------------------------------------------------------------------
// fields / values
// ---------------------------------------------------------------------------

/// 필드 하나의 요약(목록·검증이 함께 쓴다).
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldSummary {
    name: String,
    cell_field: bool,
    section: usize,
    para: usize,
    depth: usize,
}

fn summarize_fields(doc: &HwpDocument) -> Vec<FieldSummary> {
    doc.collect_all_fields()
        .iter()
        .map(|fi| FieldSummary {
            name: fi.field.field_name().unwrap_or("").to_string(),
            cell_field: fi.field.ctrl_id == 0,
            section: fi.location.section_index,
            para: fi.location.para_index,
            depth: fi.location.nested_path.len(),
        })
        .collect()
}

fn list_fields(request: &Value) -> Result<Value, String> {
    let (_, doc) = load_input(request)?;
    let option = request.get("option").and_then(Value::as_u64).unwrap_or(0);
    let number = request.get("number").and_then(Value::as_u64).unwrap_or(0);
    let all = summarize_fields(&doc);
    let selected: Vec<&FieldSummary> = all
        .iter()
        .filter(|f| !f.name.is_empty())
        .filter(|f| match option {
            0 => f.cell_field,
            1 => !f.cell_field,
            _ => true,
        })
        .collect();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    for f in &selected {
        *totals.entry(f.name.clone()).or_default() += 1;
    }
    let names: Vec<String> = selected
        .iter()
        .map(|f| {
            let n = seen.entry(f.name.clone()).or_default();
            *n += 1;
            match number {
                1 => format!("{}{{{{{}}}}}", f.name, n),
                2 => format!("{}{{{{{}}}}}", f.name, totals[&f.name]),
                _ => f.name.clone(),
            }
        })
        .collect();
    let fields: Vec<Value> = selected
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "cellField": f.cell_field,
                "section": f.section,
                "para": f.para,
                "depth": f.depth,
            })
        })
        .collect();
    Ok(json!({
        "pageCount": doc.page_count(),
        "option": option,
        "number": number,
        "count": names.len(),
        "names": names,
        "fields": fields,
    }))
}

/// 필드 이름의 현재 값. 셀 필드는 셀 전체 글자, 누름틀은 필드 값. 없으면 None.
fn field_value(doc: &HwpDocument, name: &str) -> Option<String> {
    let fields = doc.collect_all_fields();
    let fi = fields
        .iter()
        .find(|f| f.field.field_name() == Some(name))?;
    if fi.field.ctrl_id != 0 {
        return Some(fi.value.clone());
    }
    match cell_text_actions::resolve_field(doc, name) {
        Ok(slot) => match slot.cell {
            Some(c) => cell_text_actions::cell_text(doc, &slot, c).ok(),
            None => Some(fi.value.clone()),
        },
        // 깊이 2 이상의 셀 필드: 첫 문단 값만
        Err(_) => Some(fi.value.clone()),
    }
}

fn read_values(request: &Value) -> Result<Value, String> {
    let (_, doc) = load_input(request)?;
    let names: Vec<String> = request
        .get("fields")
        .and_then(Value::as_array)
        .ok_or("'fields' 배열이 필요합니다")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let mut values = serde_json::Map::new();
    for name in names {
        let v = field_value(&doc, &name)
            .map(Value::String)
            .unwrap_or(Value::Null);
        values.insert(name, v);
    }
    Ok(json!({ "pageCount": doc.page_count(), "values": values }))
}

// ---------------------------------------------------------------------------
// convert
// ---------------------------------------------------------------------------

fn process(request: &Value) -> Result<Value, String> {
    let output = str_field(request, "output")?;
    let verify = request
        .get("verify")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    // 검증 실패를 오류로 승격할지. 기본 true — 무증상 저장 손상을 운영에서 막는 장치다.
    let strict = request
        .get("strictVerify")
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

    let (bytes, mut doc) = load_input(request)?;
    let before = Snapshot::take(&doc, &actions);

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
        let report = verify_output(&out_bytes, &actions, &before);
        let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
        response["verify"] = report;
        if !ok && strict {
            let _ = fs::remove_file(output);
            response["ok"] = json!(false);
            response["error"] = json!("저장 후 재확인 실패 (verify.mismatches 참조) — 출력 파일을 지웠습니다");
        }
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

// ---------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------

/// 본문 문단의 컨트롤 종류 서명. 셀 안만 편집했으면 저장 전후가 같아야 한다 — 저장 결함 1
/// (누름틀 끝이 표 뒤로 밀림)은 다시 열었을 때 필드 범위·컨트롤 배치가 달라져 드러난다.
fn body_signature(doc: &HwpDocument) -> Vec<(usize, usize, Vec<&'static str>)> {
    doc.document()
        .sections
        .iter()
        .enumerate()
        .flat_map(|(si, s)| {
            s.paragraphs.iter().enumerate().map(move |(pi, p)| {
                let kinds: Vec<&'static str> = p
                    .controls
                    .iter()
                    .map(|c| match c {
                        Control::SectionDef(_) => "secd",
                        Control::ColumnDef(_) => "cold",
                        Control::Table(_) => "tbl",
                        Control::Picture(_) => "pic",
                        Control::Shape(_) => "gso",
                        Control::Header(_) => "head",
                        Control::Footer(_) => "foot",
                        Control::Footnote(_) => "fn",
                        Control::Endnote(_) => "en",
                        Control::Bookmark(_) => "bokm",
                        Control::Hyperlink(_) => "link",
                        _ => "etc",
                    })
                    .collect();
                (si, pi, kinds)
            })
        })
        .collect()
}

/// 액션 적용 전의 문서 사실과, 액션에서 계산한 기대값.
struct Snapshot {
    page_count: u32,
    fields: Vec<FieldSummary>,
    body: Vec<(usize, usize, Vec<&'static str>)>,
    /// 필드 → 기대 셀 문단(셀 필드) 또는 기대 값(누름틀). 액션 순서대로 누적한다.
    expected: BTreeMap<String, Vec<String>>,
    /// 필드 → 기대 그림 수(셀 필드만)
    expected_pictures: BTreeMap<String, usize>,
    /// 표 소유 문단 (section, para) → (지워질 형제 그림 수, 추가될 그림 수).
    /// 셀 비우기는 이 필드를 위해 rhwp 가 띄운 형제 그림을 지우고, 그림 액션은 형제 그림을 끝에 붙인다.
    pic_delta_in_para: BTreeMap<(usize, usize), (usize, usize)>,
}

impl Snapshot {
    fn take(doc: &HwpDocument, actions: &[CellTextAction]) -> Self {
        let mut expected: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut expected_pictures: BTreeMap<String, usize> = BTreeMap::new();
        let mut pic_delta_in_para: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
        // 같은 요청 안에서 같은 필드를 두 번 비우면 형제 그림은 한 번만 지워진다
        let mut cleared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for a in actions {
            let slot = cell_text_actions::resolve_field(doc, &a.field_name).ok();
            let is_cell = slot.as_ref().map(|s| s.cell.is_some()).unwrap_or(false);
            let lines = a.lines();
            let clears_cell = is_cell && matches!(a.kind, ActionKind::InsertTextEx | ActionKind::InsertText | ActionKind::InsertImage);
            if clears_cell && cleared.insert(a.field_name.clone()) {
                if let Some(s) = &slot {
                    let siblings = cell_text_actions::sibling_picture_count(doc, s, &a.field_name);
                    let added_so_far = pic_delta_in_para.get(&(s.section, s.para)).map(|d| d.1).unwrap_or(0);
                    let e = pic_delta_in_para.entry((s.section, s.para)).or_default();
                    // 이 요청에서 이미 붙인 그림이 있으면 그것도 지워진다
                    e.0 += siblings;
                    e.1 = e.1.saturating_sub(added_so_far.min(e.1));
                    e.0 = e.0.saturating_sub(added_so_far);
                }
            }
            match a.kind {
                ActionKind::InsertTextEx | ActionKind::InsertText => {
                    expected.insert(a.field_name.clone(), lines);
                    if is_cell {
                        expected_pictures.insert(a.field_name.clone(), 0);
                    }
                }
                ActionKind::AppendTextEx => {
                    let e = expected.entry(a.field_name.clone()).or_insert_with(|| {
                        slot.as_ref()
                            .and_then(|s| s.cell.map(|c| (s, c)))
                            .and_then(|(s, c)| cell_text_actions::cell_text(doc, s, c).ok())
                            .map(|t| t.split('\n').map(str::to_string).collect())
                            .unwrap_or_default()
                    });
                    e.extend(lines);
                }
                ActionKind::InsertImage => {
                    expected.insert(a.field_name.clone(), vec![String::new()]);
                    expected_pictures.insert(a.field_name.clone(), 1);
                    if let Some(s) = &slot {
                        pic_delta_in_para.entry((s.section, s.para)).or_default().1 += 1;
                    }
                }
                ActionKind::AppendPicture => {
                    let e = expected.entry(a.field_name.clone()).or_insert_with(|| {
                        slot.as_ref()
                            .and_then(|s| s.cell.map(|c| (s, c)))
                            .and_then(|(s, c)| cell_text_actions::cell_text(doc, s, c).ok())
                            .map(|t| t.split('\n').map(str::to_string).collect())
                            .unwrap_or_default()
                    });
                    e.push(String::new());
                    let p = expected_pictures.entry(a.field_name.clone()).or_insert_with(|| {
                        slot.as_ref()
                            .and_then(|s| s.cell.map(|c| (s, c)))
                            .map(|(s, c)| cell_text_actions::cell_picture_count(doc, s, c, &a.field_name))
                            .unwrap_or(0)
                    });
                    *p += 1;
                    if let Some(s) = &slot {
                        pic_delta_in_para.entry((s.section, s.para)).or_default().1 += 1;
                    }
                }
            }
        }
        Self {
            page_count: doc.page_count(),
            fields: summarize_fields(doc),
            body: body_signature(doc),
            expected,
            expected_pictures,
            pic_delta_in_para,
        }
    }
}

/// 저장한 바이트를 다시 열어 기대값과 대조한다.
///
/// 검사 항목: 다시 열림 · 필드 목록(이름·종류·위치) 동일 · 본문 컨트롤 배치 동일(그림 액션의
/// 소유 문단은 `pic` 추가분만 허용) · 쪽수 동일 · 필드별 셀 글자/값 일치 · 셀 그림 수 일치.
/// 하나라도 다르면 `ok:false` 와 `mismatches`.
fn verify_output(out_bytes: &[u8], actions: &[CellTextAction], before: &Snapshot) -> Value {
    let doc = match load_document(out_bytes) {
        Ok(d) => d,
        Err(_) => return json!({ "ok": false, "reloaded": false, "mismatches": ["저장본을 다시 열 수 없습니다"] }),
    };
    let mut mismatches: Vec<String> = Vec::new();

    // 쪽수
    let page_count = doc.page_count();
    if page_count != before.page_count {
        mismatches.push(format!("쪽수 변화: {} → {}", before.page_count, page_count));
    }

    // 필드 목록
    let fields_after = summarize_fields(&doc);
    if fields_after != before.fields {
        let names_b: Vec<&str> = before.fields.iter().map(|f| f.name.as_str()).collect();
        let names_a: Vec<&str> = fields_after.iter().map(|f| f.name.as_str()).collect();
        if names_a != names_b {
            mismatches.push(format!("필드 목록 변화: {} → {} 개, 이름/순서 다름", names_b.len(), names_a.len()));
        } else {
            for (b, a) in before.fields.iter().zip(fields_after.iter()) {
                if b != a {
                    mismatches.push(format!(
                        "필드 '{}' 위치/종류 변화: ({},{},depth{},cell{}) → ({},{},depth{},cell{})",
                        b.name, b.section, b.para, b.depth, b.cell_field, a.section, a.para, a.depth, a.cell_field
                    ));
                }
            }
        }
    }

    // 본문 컨트롤 배치
    let body_after = body_signature(&doc);
    if body_after.len() != before.body.len() {
        mismatches.push(format!("본문 문단 수 변화: {} → {}", before.body.len(), body_after.len()));
    } else {
        for (b, a) in before.body.iter().zip(body_after.iter()) {
            if b.2 == a.2 {
                continue;
            }
            let (removed, added) = before.pic_delta_in_para.get(&(b.0, b.1)).copied().unwrap_or((0, 0));
            // rhwp 가 띄운 형제 그림은 컨트롤 끝에 붙으므로 뒤에서부터 지워진다
            let mut expected = b.2.clone();
            let mut to_remove = removed;
            while to_remove > 0 {
                match expected.iter().rposition(|k| *k == "pic") {
                    Some(i) => {
                        expected.remove(i);
                        to_remove -= 1;
                    }
                    None => break,
                }
            }
            expected.extend(std::iter::repeat("pic").take(added));
            if a.2 != expected {
                mismatches.push(format!(
                    "구역 {} 문단 {} 컨트롤 배치 변화: {:?} → {:?}",
                    b.0, b.1, b.2, a.2
                ));
            }
        }
    }

    // 필드별 값
    let mut field_values = serde_json::Map::new();
    let mut cell_texts = serde_json::Map::new();
    let mut pictures = serde_json::Map::new();
    let mut seen = std::collections::BTreeSet::new();
    for action in actions {
        if !seen.insert(action.field_name.clone()) {
            continue;
        }
        let name = &action.field_name;
        let expected = before.expected.get(name).cloned().unwrap_or_default();
        match cell_text_actions::resolve_field(&doc, name) {
            Ok(slot) => match slot.cell {
                Some(c) => {
                    let text = cell_text_actions::cell_text(&doc, &slot, c).unwrap_or_default();
                    let paras: Vec<String> = text.split('\n').map(str::to_string).collect();
                    field_values.insert(name.clone(), json!(paras.first().cloned().unwrap_or_default()));
                    cell_texts.insert(name.clone(), json!(paras));
                    if paras != expected {
                        mismatches.push(format!("'{name}' 셀 글자 불일치: 기대 {expected:?}, 실제 {paras:?}"));
                    }
                    let count = cell_text_actions::cell_picture_count(&doc, &slot, c, name);
                    pictures.insert(name.clone(), json!(count));
                    if let Some(exp) = before.expected_pictures.get(name) {
                        if *exp != count {
                            mismatches.push(format!("'{name}' 셀 그림 수 불일치: 기대 {exp}, 실제 {count}"));
                        }
                    }
                }
                None => {
                    let v = field_value(&doc, name).unwrap_or_default();
                    field_values.insert(name.clone(), json!(v));
                    let exp = expected.join("\n");
                    if v != exp {
                        mismatches.push(format!("'{name}' 누름틀 값 불일치: 기대 {exp:?}, 실제 {v:?}"));
                    }
                }
            },
            Err(e) => {
                field_values.insert(name.clone(), Value::Null);
                mismatches.push(format!("'{name}' 저장본에서 찾지 못함: {e}"));
            }
        }
    }

    json!({
        "ok": mismatches.is_empty(),
        "reloaded": true,
        "pageCount": page_count,
        "pageCountBefore": before.page_count,
        "fieldCount": fields_after.len(),
        "fieldValues": field_values,
        "cellTexts": cell_texts,
        "pictures": pictures,
        "mismatches": mismatches,
    })
}
