//! BMS 문서변환 액션(`insertTextEx`·`appendTextEx`)을 rhwp 코어로 수행한다. [POC]
//!
//! BMS(`com.hs.gwweb.converter`)는 결재 서명을 셀 필드 하나에 액션 두 개로 넣는다 —
//! `insertTextEx`(셀 내용 교체, 이름)와 `appendTextEx`(새 문단 추가, 날짜). 각 액션은
//! 글꼴 이름·크기(pt)·색(`0x00BBGGRR`)·굵게를 따로 가진다. 이 모듈은 그 두 액션만 다루고,
//! 값의 `\r`(한컴 문단 나눔)은 새 문단으로 푼다.
//!
//! 범위(POC): 깊이 1 표의 셀 필드. 누름틀 필드는 값만 바꾸고 글꼴은 적용하지 않는다.

use rhwp::document_core::queries::field_query::NestedEntry;
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

/// 한 액션의 입력. BMS `CellAction` 과 같은 이름을 쓴다(값은 전부 문자열로 와도 된다).
#[derive(Debug, Clone)]
pub(crate) struct CellTextAction {
    pub action: String,
    pub field_name: String,
    pub input_value: String,
    pub font_name: Option<String>,
    /// pt 단위
    pub font_size: Option<i32>,
    /// `0x00BBGGRR`(BMS) 또는 `#RRGGBB`
    pub font_color: Option<String>,
    pub font_bold: Option<bool>,
}

impl CellTextAction {
    /// JSON 객체에서 읽는다. 숫자·불리언은 문자열로 와도 받는다(BMS 는 전부 문자열이다).
    pub(crate) fn from_json(v: &Value) -> Result<Self, String> {
        let obj = v.as_object().ok_or("액션은 JSON 객체여야 합니다")?;
        let text = |key: &str| -> Option<String> {
            match obj.get(key) {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Number(n)) => Some(n.to_string()),
                Some(Value::Bool(b)) => Some(b.to_string()),
                _ => None,
            }
        };
        let action = text("action").unwrap_or_default();
        if action != "insertTextEx" && action != "appendTextEx" {
            return Err(format!(
                "지원하지 않는 액션: {action:?} (insertTextEx·appendTextEx 만)"
            ));
        }
        let field_name = text("fieldName")
            .filter(|s| !s.is_empty())
            .ok_or("fieldName 이 비었습니다")?;
        let font_size = match text("fontSize") {
            Some(s) if !s.is_empty() => Some(
                s.trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("fontSize 는 양의 정수(pt)여야 합니다: {s}"))?,
            ),
            _ => None,
        };
        let font_bold = match text("fontBold") {
            Some(s) if !s.is_empty() => Some(matches!(s.trim(), "1" | "true" | "TRUE")),
            _ => None,
        };
        Ok(Self {
            action,
            field_name,
            input_value: text("inputValue").unwrap_or_default(),
            font_name: text("fontName").filter(|s| !s.is_empty()),
            font_size,
            font_color: text("fontColor").filter(|s| !s.is_empty()),
            font_bold,
        })
    }

    /// 값을 문단 단위로 나눈다. 한컴 변환기는 `\r` 을 문단 나눔으로 받는다.
    fn lines(&self) -> Vec<String> {
        self.input_value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(str::to_string)
            .collect()
    }

    /// rhwp `apply-char-format` 속성 JSON. 지정한 속성만 담는다.
    fn char_props(&self, doc: &mut HwpDocument) -> Option<String> {
        let mut props = serde_json::Map::new();
        if let Some(name) = &self.font_name {
            let id = doc.find_or_create_font_id_native(name);
            props.insert("fontId".into(), json!(id));
        }
        if let Some(pt) = self.font_size {
            props.insert("fontSize".into(), json!(pt * 100));
        }
        if let Some(css) = self.font_color.as_deref().and_then(color_to_css) {
            props.insert("textColor".into(), json!(css));
        }
        if let Some(b) = self.font_bold {
            props.insert("bold".into(), json!(b));
        }
        if props.is_empty() {
            None
        } else {
            Some(Value::Object(props).to_string())
        }
    }
}

/// `0x00BBGGRR`(COLORREF) 또는 `#RRGGBB` 를 rhwp 가 받는 `#RRGGBB` 로.
fn color_to_css(raw: &str) -> Option<String> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| s.to_string());
    }
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let v = u32::from_str_radix(hex, 16).ok()?;
    let (r, g, b) = (v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF);
    Some(format!("#{r:02X}{g:02X}{b:02X}"))
}

/// 필드가 든 셀의 자리.
struct CellSlot {
    section: usize,
    para: usize,
    ctrl: usize,
    cell: usize,
    /// 필드가 가리키는 셀 문단
    cell_para: usize,
    is_cell_field: bool,
}

fn resolve_cell(doc: &HwpDocument, field_name: &str) -> Result<CellSlot, String> {
    let fields = doc.collect_all_fields();
    let fi = fields
        .iter()
        .find(|f| {
            f.field
                .field_name()
                .map(|n| n == field_name)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("필드 이름 '{field_name}' 없음"))?;
    let loc = &fi.location;
    let [NestedEntry::TableCell {
        control_index,
        cell_index,
        para_index,
    }] = loc.nested_path.as_slice()
    else {
        return Err(format!(
            "필드 '{field_name}' 는 깊이 1 표 셀이 아닙니다 (POC 범위 밖, 경로 깊이 {})",
            loc.nested_path.len()
        ));
    };
    Ok(CellSlot {
        section: loc.section_index,
        para: loc.para_index,
        ctrl: *control_index,
        cell: *cell_index,
        cell_para: *para_index,
        is_cell_field: fi.field.ctrl_id == 0,
    })
}

fn cell_para_lens(doc: &HwpDocument, slot: &CellSlot) -> Result<Vec<usize>, String> {
    use rhwp::model::control::Control;
    let document = doc.document();
    let paragraph = document
        .sections
        .get(slot.section)
        .and_then(|s| s.paragraphs.get(slot.para))
        .ok_or("셀 소유 문단 없음")?;
    let Some(Control::Table(table)) = paragraph.controls.get(slot.ctrl) else {
        return Err("필드 위치의 컨트롤이 표가 아닙니다".into());
    };
    let cell = table.cells.get(slot.cell).ok_or("셀 인덱스 초과")?;
    Ok(cell
        .paragraphs
        .iter()
        .map(|p| p.text.chars().count())
        .collect())
}

fn apply_props(
    doc: &mut HwpDocument,
    slot: &CellSlot,
    cell_para: usize,
    len: usize,
    props: Option<&str>,
) -> Result<(), String> {
    let (Some(props), true) = (props, len > 0) else {
        return Ok(());
    };
    doc.apply_char_format_in_cell_native(
        slot.section,
        slot.para,
        slot.ctrl,
        slot.cell,
        cell_para,
        0,
        len,
        props,
    )
    .map(|_| ())
    .map_err(|e| format!("글꼴 적용 실패: {e}"))
}

/// 셀 끝에 문단을 하나 붙이고 글자를 넣는다.
fn append_paragraph(
    doc: &mut HwpDocument,
    slot: &CellSlot,
    text: &str,
    props: Option<&str>,
) -> Result<usize, String> {
    let lens = cell_para_lens(doc, slot)?;
    let last = lens.len().checked_sub(1).ok_or("셀에 문단이 없습니다")?;
    doc.split_paragraph_in_cell_native(
        slot.section,
        slot.para,
        slot.ctrl,
        slot.cell,
        last,
        lens[last],
        None,
    )
    .map_err(|e| format!("문단 추가 실패: {e}"))?;
    let new_idx = last + 1;
    if !text.is_empty() {
        doc.insert_text_in_cell_native(
            slot.section,
            slot.para,
            slot.ctrl,
            slot.cell,
            new_idx,
            0,
            text,
        )
        .map_err(|e| format!("문단 글자 삽입 실패: {e}"))?;
    }
    apply_props(doc, slot, new_idx, text.chars().count(), props)?;
    Ok(new_idx)
}

/// 액션 하나를 수행한다. 성공하면 결과 요약 JSON 을 돌려준다.
pub(crate) fn apply(doc: &mut HwpDocument, action: &CellTextAction) -> Result<Value, String> {
    let slot = resolve_cell(doc, &action.field_name)?;
    let lines = action.lines();
    let props = action.char_props(doc);
    let mut notes: Vec<String> = Vec::new();
    let mut paragraphs: Vec<usize> = Vec::new();

    match action.action.as_str() {
        "insertTextEx" => {
            // 첫 줄은 필드 값 교체(검증된 코어 경로). 셀 필드는 셀의 첫 문단 텍스트가 바뀐다.
            let first = &lines[0];
            doc.set_field_value_by_name_at(&action.field_name, 0, first)
                .map_err(|e| format!("필드 값 교체 실패: {e}"))?;
            if slot.is_cell_field {
                apply_props(
                    doc,
                    &slot,
                    slot.cell_para,
                    first.chars().count(),
                    props.as_deref(),
                )?;
                paragraphs.push(slot.cell_para);
            } else if props.is_some() {
                notes.push("누름틀 필드에는 글꼴을 적용하지 않았습니다 (POC 범위 밖)".into());
            }
            for line in &lines[1..] {
                if !slot.is_cell_field {
                    return Err(
                        "누름틀 필드에는 여러 줄 값을 넣을 수 없습니다 (POC 범위 밖)".into(),
                    );
                }
                paragraphs.push(append_paragraph(doc, &slot, line, props.as_deref())?);
            }
        }
        "appendTextEx" => {
            if !slot.is_cell_field {
                return Err("appendTextEx 는 셀 필드에만 쓸 수 있습니다".into());
            }
            for line in &lines {
                paragraphs.push(append_paragraph(doc, &slot, line, props.as_deref())?);
            }
        }
        other => return Err(format!("지원하지 않는 액션: {other}")),
    }

    Ok(json!({
        "action": action.action,
        "fieldName": action.field_name,
        "ok": true,
        "cell": { "section": slot.section, "para": slot.para, "ctrl": slot.ctrl, "cell": slot.cell },
        "paragraphs": paragraphs,
        "charProps": props.as_deref().and_then(|p| serde_json::from_str::<Value>(p).ok()),
        "notes": notes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_to_css() {
        assert_eq!(color_to_css("0x00000000").as_deref(), Some("#000000"));
        assert_eq!(color_to_css("0x00FF0000").as_deref(), Some("#0000FF")); // BBGGRR → RRGGBB
        assert_eq!(color_to_css("#1A2B3C").as_deref(), Some("#1A2B3C"));
        assert_eq!(color_to_css("red"), None);
    }

    #[test]
    fn parses_bms_action_strings() {
        let v = json!({"action":"appendTextEx","fieldName":"서명.2","inputValue":"2026/10/29",
            "fontName":"굴림","fontSize":"7","fontColor":"0x00000000","fontBold":"1","appendLine":"1"});
        let a = CellTextAction::from_json(&v).unwrap();
        assert_eq!(a.font_size, Some(7));
        assert_eq!(a.font_bold, Some(true));
        assert_eq!(a.lines(), vec!["2026/10/29"]);
        let bad = json!({"action":"insertText","fieldName":"x"});
        assert!(CellTextAction::from_json(&bad).is_err());
    }

    #[test]
    fn cr_splits_paragraphs() {
        let a = CellTextAction::from_json(
            &json!({"action":"insertTextEx","fieldName":"서명.1","inputValue":"10/29\r홍길동"}),
        )
        .unwrap();
        assert_eq!(a.lines(), vec!["10/29", "홍길동"]);
    }
}
