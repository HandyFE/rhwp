//! BMS 문서변환 액션을 rhwp 코어로 수행한다. (rhwp converter)
//!
//! BMS(`com.hs.gwweb.converter`)가 `DocConverter.convert()` 에 넘기는 `CellAction` 을 그대로
//! 받는다. 액션 이름과 의미는 hwplib 구현체(`HwpLibConverterService`)와 한컴 변환기를 따른다.
//!
//! | 액션 | 의미 |
//! |---|---|
//! | `insertTextEx` (`insertSignTextEx`·`insertSignDateTextEx`) | 셀 필드면 셀 내용 전체를 지우고 글자를 넣는다(글꼴 적용). 누름틀이면 값만 바꾼다. 빈 값은 "비우기"다(문서번호·관인 셀) |
//! | `appendTextEx` (`appendSign…`) | 셀 끝에 문단을 추가하고 글자를 넣는다(글꼴 적용) |
//! | `insertText` | `insertTextEx` 와 같되 글꼴을 바꾸지 않는다(결재완료일·`miskey`·본문 누름틀) |
//! | `insertImage` | 셀을 비우고 그림을 셀 크기에 맞춰 넣는다(서명 이미지·관인) |
//! | `appendPicture` | 셀 끝에 문단을 추가하고 그 자리에 그림을 넣는다(날짜 위 정렬 서명) |
//!
//! 값의 `\r`(한컴 문단 나눔)은 새 문단으로 푼다. 그림 파일은 호스트(`rhwp.war`)가 임시 경로
//! `imagePath` 로 준다. 그림 크기는 셀 안쪽(여백 제외)에 비율을 유지해 맞추되, `imageWidthMm`·
//! `imageHeightMm` 가 오면 그 상자 안에 맞춘다. hwplib 는 16×7mm 고정 사각형이었다.
//!
//! 범위: 깊이 1 표의 셀 필드. 누름틀은 값만 바꾼다(글꼴·그림·여러 줄 불가).

use rhwp::document_core::queries::field_query::NestedEntry;
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

/// 1mm = 7200/25.4 HWPUNIT.
const HWPUNIT_PER_MM: f64 = 7200.0 / 25.4;
/// 96dpi 픽셀 1개 = 75 HWPUNIT (코어의 crop 환산비와 같다).
const HWPUNIT_PER_PX: f64 = 75.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionKind {
    InsertTextEx,
    AppendTextEx,
    InsertText,
    InsertImage,
    AppendPicture,
}

impl ActionKind {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "insertTextEx" | "insertSignTextEx" | "insertSignDateTextEx" => Self::InsertTextEx,
            "appendTextEx" | "appendSignTextEx" | "appendSignDateTextEx" => Self::AppendTextEx,
            "insertText" => Self::InsertText,
            "insertImage" => Self::InsertImage,
            "appendPicture" => Self::AppendPicture,
            _ => return None,
        })
    }

    pub(crate) fn is_image(self) -> bool {
        matches!(self, Self::InsertImage | Self::AppendPicture)
    }
}

/// 한 액션의 입력. BMS `CellAction` 과 같은 이름을 쓴다(값은 전부 문자열로 와도 된다).
#[derive(Debug, Clone)]
pub(crate) struct CellTextAction {
    pub kind: ActionKind,
    pub action: String,
    pub field_name: String,
    pub input_value: String,
    pub font_name: Option<String>,
    /// pt 단위
    pub font_size: Option<i32>,
    /// `0x00BBGGRR`(BMS) 또는 `#RRGGBB`
    pub font_color: Option<String>,
    pub font_bold: Option<bool>,
    /// 그림 파일 경로(호스트가 준비). 그림 액션에만 있다.
    pub image_path: Option<String>,
    /// 셀 안쪽 대비 맞춤 비율(0 초과 1 이하). 기본 1.0.
    pub image_fit: f64,
    /// 명시 상자 크기(mm). 둘 다 있으면 셀 대신 이 상자에 맞춘다.
    pub image_box_mm: Option<(f64, f64)>,
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
        let kind = ActionKind::parse(&action).ok_or_else(|| {
            format!(
                "지원하지 않는 액션: {action:?} (insertTextEx·appendTextEx·insertText·insertImage·appendPicture)"
            )
        })?;
        let field_name = text("fieldName")
            .filter(|s| !s.is_empty())
            .ok_or("fieldName 이 비었습니다")?;
        let font_size = match text("fontSize") {
            Some(s) if !s.trim().is_empty() => Some(
                s.trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("fontSize 는 양의 정수(pt)여야 합니다: {s}"))?,
            ),
            _ => None,
        };
        let font_bold = match text("fontBold") {
            Some(s) if !s.trim().is_empty() => Some(matches!(s.trim(), "1" | "true" | "TRUE")),
            _ => None,
        };
        let image_path = text("imagePath").filter(|s| !s.is_empty());
        if kind.is_image() && image_path.is_none() {
            return Err(format!("{action} 에는 imagePath 가 필요합니다 (호스트가 그림 파일을 준비)"));
        }
        let num = |key: &str| -> Result<Option<f64>, String> {
            match text(key) {
                Some(s) if !s.trim().is_empty() => s
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| *n > 0.0)
                    .map(Some)
                    .ok_or_else(|| format!("{key} 는 양수여야 합니다: {s}")),
                _ => Ok(None),
            }
        };
        let image_fit = num("imageFit")?.unwrap_or(1.0).min(1.0);
        let image_box_mm = match (num("imageWidthMm")?, num("imageHeightMm")?) {
            (Some(w), Some(h)) => Some((w, h)),
            (None, None) => None,
            _ => return Err("imageWidthMm 와 imageHeightMm 는 함께 줘야 합니다".into()),
        };
        Ok(Self {
            kind,
            action,
            field_name,
            input_value: text("inputValue").unwrap_or_default(),
            font_name: text("fontName").filter(|s| !s.is_empty()),
            font_size,
            font_color: text("fontColor").filter(|s| !s.is_empty()),
            font_bold,
            image_path,
            image_fit,
            image_box_mm,
        })
    }

    /// 값을 문단 단위로 나눈다. 한컴 변환기는 `\r` 을 문단 나눔으로 받는다.
    pub(crate) fn lines(&self) -> Vec<String> {
        self.input_value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(str::to_string)
            .collect()
    }

    /// rhwp `apply-char-format` 속성 JSON. 지정한 속성만 담는다.
    fn char_props(&self, doc: &mut HwpDocument) -> Option<String> {
        if self.kind == ActionKind::InsertText || self.kind.is_image() {
            return None;
        }
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

/// 필드가 든 자리.
#[derive(Debug, Clone)]
pub(crate) struct FieldSlot {
    pub section: usize,
    pub para: usize,
    /// 셀 필드일 때 표 컨트롤·셀·셀 문단
    pub cell: Option<CellRef>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CellRef {
    pub ctrl: usize,
    pub cell: usize,
    pub cell_para: usize,
}

impl FieldSlot {
    fn cell(&self, field_name: &str, what: &str) -> Result<CellRef, String> {
        self.cell
            .ok_or_else(|| format!("{what} 는 셀 필드에만 쓸 수 있습니다 (필드 '{field_name}' 는 누름틀)"))
    }
}

pub(crate) fn resolve_field(doc: &HwpDocument, field_name: &str) -> Result<FieldSlot, String> {
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
    let is_cell_field = fi.field.ctrl_id == 0;
    if !is_cell_field {
        return Ok(FieldSlot {
            section: loc.section_index,
            para: loc.para_index,
            cell: None,
        });
    }
    let [NestedEntry::TableCell {
        control_index,
        cell_index,
        para_index,
    }] = loc.nested_path.as_slice()
    else {
        return Err(format!(
            "셀 필드 '{field_name}' 는 깊이 1 표 셀이 아닙니다 (지원 범위 밖, 경로 깊이 {})",
            loc.nested_path.len()
        ));
    };
    Ok(FieldSlot {
        section: loc.section_index,
        para: loc.para_index,
        cell: Some(CellRef {
            ctrl: *control_index,
            cell: *cell_index,
            cell_para: *para_index,
        }),
    })
}

/// 셀의 문단별 글자 수(코드 유닛이 아니라 문자 수 — `insert_text_in_cell_native` 의 오프셋 단위).
fn cell_para_lens(doc: &HwpDocument, slot: &FieldSlot, c: CellRef) -> Result<Vec<usize>, String> {
    Ok(cell_paragraphs(doc, slot, c)?
        .iter()
        .map(|p| p.text.chars().count())
        .collect())
}

fn cell_paragraphs<'a>(
    doc: &'a HwpDocument,
    slot: &FieldSlot,
    c: CellRef,
) -> Result<&'a [rhwp::model::paragraph::Paragraph], String> {
    let document = doc.document();
    let paragraph = document
        .sections
        .get(slot.section)
        .and_then(|s| s.paragraphs.get(slot.para))
        .ok_or("셀 소유 문단 없음")?;
    let Some(Control::Table(table)) = paragraph.controls.get(c.ctrl) else {
        return Err("필드 위치의 컨트롤이 표가 아닙니다".into());
    };
    let cell = table.cells.get(c.cell).ok_or("셀 인덱스 초과")?;
    Ok(&cell.paragraphs)
}

/// 셀의 전체 글자(문단은 `\n` 으로 잇는다). 검증과 `getFieldValues` 가 쓴다.
pub(crate) fn cell_text(doc: &HwpDocument, slot: &FieldSlot, c: CellRef) -> Result<String, String> {
    Ok(cell_paragraphs(doc, slot, c)?
        .iter()
        .map(|p| p.text.clone())
        .collect::<Vec<_>>()
        .join("\n"))
}

/// 셀 안 문단 컨트롤 중 그림·도형 수 + 이 셀을 위해 rhwp 가 띄운 형제 그림 수.
pub(crate) fn cell_picture_count(doc: &HwpDocument, slot: &FieldSlot, c: CellRef, field_name: &str) -> usize {
    let inner = cell_paragraphs(doc, slot, c)
        .map(|ps| {
            ps.iter()
                .flat_map(|p| p.controls.iter())
                .filter(|k| matches!(k, Control::Picture(_) | Control::Shape(_)))
                .count()
        })
        .unwrap_or(0);
    inner + sibling_pictures_for(doc, slot, field_name).len()
}

/// 이 필드를 위해 rhwp 가 띄운 형제 그림 수 (검증의 기대값 계산용).
pub(crate) fn sibling_picture_count(doc: &HwpDocument, slot: &FieldSlot, field_name: &str) -> usize {
    sibling_pictures_for(doc, slot, field_name).len()
}

/// 표가 든 문단에서 이 필드를 위해 rhwp 가 띄운 그림(설명이 필드 이름)의 컨트롤 인덱스.
fn sibling_pictures_for(doc: &HwpDocument, slot: &FieldSlot, field_name: &str) -> Vec<usize> {
    doc.document()
        .sections
        .get(slot.section)
        .and_then(|s| s.paragraphs.get(slot.para))
        .map(|p| {
            p.controls
                .iter()
                .enumerate()
                .filter_map(|(i, k)| match k {
                    Control::Picture(pic)
                        if !pic.common.treat_as_char && pic.common.description == field_name =>
                    {
                        Some(i)
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn apply_props(
    doc: &mut HwpDocument,
    slot: &FieldSlot,
    c: CellRef,
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
        c.ctrl,
        c.cell,
        cell_para,
        0,
        len,
        props,
    )
    .map(|_| ())
    .map_err(|e| format!("글꼴 적용 실패: {e}"))
}

/// 셀 끝에 문단을 하나 붙이고 글자를 넣는다. 새 문단 번호를 돌려준다.
fn append_paragraph(
    doc: &mut HwpDocument,
    slot: &FieldSlot,
    c: CellRef,
    text: &str,
    props: Option<&str>,
) -> Result<usize, String> {
    let lens = cell_para_lens(doc, slot, c)?;
    let last = lens.len().checked_sub(1).ok_or("셀에 문단이 없습니다")?;
    doc.split_paragraph_in_cell_native(
        slot.section,
        slot.para,
        c.ctrl,
        c.cell,
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
            c.ctrl,
            c.cell,
            new_idx,
            0,
            text,
        )
        .map_err(|e| format!("문단 글자 삽입 실패: {e}"))?;
    }
    apply_props(doc, slot, c, new_idx, text.chars().count(), props)?;
    Ok(new_idx)
}

/// 셀을 비운다: 모든 문단의 컨트롤(그림·도형·중첩 표)을 지우고, 문단을 하나로 합친 뒤
/// 글자를 지운다. 이 필드를 위해 rhwp 가 띄운 형제 그림도 지운다. 셀 배경 그림(테두리/배경
/// 채우기)은 건드리지 않는다.
fn clear_cell(doc: &mut HwpDocument, slot: &FieldSlot, c: CellRef, field_name: &str) -> Result<Value, String> {
    let mut removed_controls = 0usize;
    let para_count = cell_para_lens(doc, slot, c)?.len();
    for idx in (0..para_count).rev() {
        let ctrl_count = cell_paragraphs(doc, slot, c)?
            .get(idx)
            .map(|p| p.controls.len())
            .unwrap_or(0);
        for ci in (0..ctrl_count).rev() {
            doc.delete_control_in_cell_native(slot.section, slot.para, c.ctrl, c.cell, idx, ci)
                .map_err(|e| format!("셀 문단 {idx} 컨트롤 {ci} 삭제 실패: {e}"))?;
            removed_controls += 1;
        }
        if idx > 0 {
            doc.merge_paragraph_in_cell_native(slot.section, slot.para, c.ctrl, c.cell, idx)
                .map_err(|e| format!("셀 문단 {idx} 병합 실패: {e}"))?;
        }
    }
    let len = cell_para_lens(doc, slot, c)?.first().copied().unwrap_or(0);
    if len > 0 {
        doc.delete_text_in_cell_native(slot.section, slot.para, c.ctrl, c.cell, 0, 0, len)
            .map_err(|e| format!("셀 글자 삭제 실패: {e}"))?;
    }
    let mut removed_siblings = 0usize;
    for idx in sibling_pictures_for(doc, slot, field_name).into_iter().rev() {
        doc.delete_picture_control_native(slot.section, slot.para, idx)
            .map_err(|e| format!("형제 그림 {idx} 삭제 실패: {e}"))?;
        removed_siblings += 1;
    }
    Ok(json!({
        "paragraphsMerged": para_count.saturating_sub(1),
        "controlsRemoved": removed_controls,
        "siblingPicturesRemoved": removed_siblings,
        "charsRemoved": len,
    }))
}

/// 셀 안쪽 크기(HWPUNIT, 여백 제외).
fn cell_inner_size(doc: &HwpDocument, slot: &FieldSlot, c: CellRef) -> Result<(f64, f64), String> {
    let document = doc.document();
    let paragraph = document
        .sections
        .get(slot.section)
        .and_then(|s| s.paragraphs.get(slot.para))
        .ok_or("셀 소유 문단 없음")?;
    let Some(Control::Table(table)) = paragraph.controls.get(c.ctrl) else {
        return Err("필드 위치의 컨트롤이 표가 아닙니다".into());
    };
    let cell = table.cells.get(c.cell).ok_or("셀 인덱스 초과")?;
    let w = cell.width as f64 - (cell.padding.left as f64 + cell.padding.right as f64);
    let h = cell.height as f64 - (cell.padding.top as f64 + cell.padding.bottom as f64);
    Ok((w.max(1.0), h.max(1.0)))
}

/// 그림을 셀 문단 `cell_para` 앞에 넣는다. 크기는 상자에 비율 유지로 맞춘다.
fn insert_picture(
    doc: &mut HwpDocument,
    slot: &FieldSlot,
    c: CellRef,
    cell_para: usize,
    action: &CellTextAction,
) -> Result<Value, String> {
    let path = action.image_path.as_deref().ok_or("imagePath 없음")?;
    let bytes = std::fs::read(path).map_err(|e| format!("그림 파일을 읽을 수 없습니다 - {path}: {e}"))?;
    let (px_w, px_h) = super::edit::media::insert_image_dimensions(&bytes)
        .ok_or("그림 형식을 알 수 없습니다 (PNG·JPEG·BMP·TIFF)")?;
    let ext = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::Jpeg) => "jpg",
        Ok(image::ImageFormat::Bmp) => "bmp",
        Ok(image::ImageFormat::Tiff) => "tif",
        _ => "png",
    };
    let (box_w, box_h) = match action.image_box_mm {
        Some((w, h)) => (w * HWPUNIT_PER_MM, h * HWPUNIT_PER_MM),
        None => {
            let (w, h) = cell_inner_size(doc, slot, c)?;
            (w * action.image_fit, h * action.image_fit)
        }
    };
    let nat_w = px_w as f64 * HWPUNIT_PER_PX;
    let nat_h = px_h as f64 * HWPUNIT_PER_PX;
    let scale = (box_w / nat_w).min(box_h / nat_h);
    let width = (nat_w * scale).round().max(1.0) as u32;
    let height = (nat_h * scale).round().max(1.0) as u32;
    let cell_path = [(c.ctrl, c.cell, cell_para)];
    doc.insert_picture_native(
        slot.section,
        slot.para,
        0,
        &cell_path,
        &bytes,
        width,
        height,
        px_w,
        px_h,
        ext,
        &action.field_name,
        None,
        None,
    )
    .map_err(|e| format!("그림 삽입 실패: {e}"))?;
    Ok(json!({
        "imageBytes": bytes.len(),
        "naturalPx": [px_w, px_h],
        "sizeHwpUnit": [width, height],
        "boxHwpUnit": [box_w.round(), box_h.round()],
        "cellPara": cell_para,
    }))
}

/// 액션 하나를 수행한다. 성공하면 결과 요약 JSON 을 돌려준다.
pub(crate) fn apply(doc: &mut HwpDocument, action: &CellTextAction) -> Result<Value, String> {
    let slot = resolve_field(doc, &action.field_name)?;
    let lines = action.lines();
    let props = action.char_props(doc);
    let mut notes: Vec<String> = Vec::new();
    let mut paragraphs: Vec<usize> = Vec::new();
    let mut extra = serde_json::Map::new();

    match action.kind {
        ActionKind::InsertTextEx | ActionKind::InsertText => {
            match slot.cell {
                Some(c) => {
                    // 셀 필드: hwplib·한컴과 같이 셀 내용 전체를 교체한다.
                    let cleared = clear_cell(doc, &slot, c, &action.field_name)?;
                    extra.insert("cleared".into(), cleared);
                    let first = &lines[0];
                    if !first.is_empty() {
                        doc.insert_text_in_cell_native(
                            slot.section, slot.para, c.ctrl, c.cell, 0, 0, first,
                        )
                        .map_err(|e| format!("셀 글자 삽입 실패: {e}"))?;
                    }
                    apply_props(doc, &slot, c, 0, first.chars().count(), props.as_deref())?;
                    paragraphs.push(0);
                    for line in &lines[1..] {
                        paragraphs.push(append_paragraph(doc, &slot, c, line, props.as_deref())?);
                    }
                }
                None => {
                    if lines.len() > 1 {
                        return Err(format!(
                            "누름틀 필드 '{}' 에는 여러 줄 값을 넣을 수 없습니다",
                            action.field_name
                        ));
                    }
                    doc.set_field_value_by_name_at(&action.field_name, 0, &lines[0])
                        .map_err(|e| format!("필드 값 교체 실패: {e}"))?;
                    if props.is_some() {
                        notes.push("누름틀 필드에는 글꼴을 적용하지 않았습니다".into());
                    }
                }
            }
        }
        ActionKind::AppendTextEx => {
            let c = slot.cell(&action.field_name, "appendTextEx")?;
            for line in &lines {
                paragraphs.push(append_paragraph(doc, &slot, c, line, props.as_deref())?);
            }
        }
        ActionKind::InsertImage => {
            let c = slot.cell(&action.field_name, "insertImage")?;
            let cleared = clear_cell(doc, &slot, c, &action.field_name)?;
            extra.insert("cleared".into(), cleared);
            let pic = insert_picture(doc, &slot, c, 0, action)?;
            extra.insert("picture".into(), pic);
            paragraphs.push(0);
        }
        ActionKind::AppendPicture => {
            let c = slot.cell(&action.field_name, "appendPicture")?;
            let new_idx = append_paragraph(doc, &slot, c, "", None)?;
            let pic = insert_picture(doc, &slot, c, new_idx, action)?;
            extra.insert("picture".into(), pic);
            paragraphs.push(new_idx);
        }
    }

    let mut out = json!({
        "action": action.action,
        "fieldName": action.field_name,
        "ok": true,
        "cellField": slot.cell.is_some(),
        "cell": slot.cell.map(|c| json!({ "section": slot.section, "para": slot.para, "ctrl": c.ctrl, "cell": c.cell })),
        "paragraphs": paragraphs,
        "charProps": props.as_deref().and_then(|p| serde_json::from_str::<Value>(p).ok()),
        "notes": notes,
    });
    if let Value::Object(map) = &mut out {
        map.extend(extra);
    }
    Ok(out)
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
        let bad = json!({"action":"addPageCell","fieldName":"x"});
        assert!(CellTextAction::from_json(&bad).is_err());
    }

    #[test]
    fn sign_aliases_and_image_requirements() {
        let a = CellTextAction::from_json(&json!({"action":"insertSignTextEx","fieldName":"서명.1","inputValue":"홍"})).unwrap();
        assert_eq!(a.kind, ActionKind::InsertTextEx);
        let t = CellTextAction::from_json(&json!({"action":"insertText","fieldName":"miskey","inputValue":"K1","fontSize":"10"})).unwrap();
        assert_eq!(t.kind, ActionKind::InsertText);
        assert!(CellTextAction::from_json(&json!({"action":"insertImage","fieldName":"서명.1"})).is_err());
        let i = CellTextAction::from_json(&json!({"action":"appendPicture","fieldName":"서명.1","imagePath":"x.png","imageWidthMm":"16","imageHeightMm":"7"})).unwrap();
        assert_eq!(i.image_box_mm, Some((16.0, 7.0)));
        assert!(CellTextAction::from_json(&json!({"action":"insertImage","fieldName":"서명.1","imagePath":"x.png","imageWidthMm":"16"})).is_err());
    }

    #[test]
    fn cr_splits_paragraphs() {
        let a = CellTextAction::from_json(
            &json!({"action":"insertTextEx","fieldName":"서명.1","inputValue":"10/29\r홍길동"}),
        )
        .unwrap();
        assert_eq!(a.lines(), vec!["10/29", "홍길동"]);
        let empty = CellTextAction::from_json(&json!({"action":"insertTextEx","fieldName":"관인","inputValue":""})).unwrap();
        assert_eq!(empty.lines(), vec![""]);
    }
}
