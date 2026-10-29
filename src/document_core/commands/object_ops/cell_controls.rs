//! 셀 문단 안의 컨트롤 삭제 — 서버 편집(BMS 관인·서명 셀 비우기)용.
//!
//! `delete_picture_control_native` 는 본문 문단의 그림만 지운다. 셀 안에 든 그림·도형
//! (한컴·hwplib 가 셀 문단 컨트롤로 넣은 서명·관인)을 지우려면 셀 문단에서 같은 일을
//! 해야 한다. 갭 닫기(뒤따르는 `char_offsets` 8 유닛 당김, `char_count` 감소, 필드 범위의
//! `control_idx` 보정)는 컨트롤 종류와 무관하므로 어느 컨트롤이든 받는다.

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::paragraph::Paragraph;

impl DocumentCore {
    /// 셀 문단 `cell_para_idx` 의 `control_idx` 번째 컨트롤을 지운다.
    pub fn delete_control_in_cell_native(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        table_control_idx: usize,
        cell_idx: usize,
        cell_para_idx: usize,
        control_idx: usize,
    ) -> Result<String, HwpError> {
        {
            // `styles`/`dpi` 와 문단을 동시에 빌리기 위해 문서 필드로 직접 내려간다.
            let para = cell_paragraph_mut(
                &mut self.document,
                section_idx,
                parent_para_idx,
                table_control_idx,
                cell_idx,
                cell_para_idx,
            )?;
            if control_idx >= para.controls.len() {
                return Err(HwpError::RenderError(format!(
                    "셀 문단 컨트롤 인덱스 {} 범위 초과 (총 {}개)",
                    control_idx,
                    para.controls.len()
                )));
            }
            remove_control_from_paragraph(para, control_idx);
            Self::reflow_paragraph_line_segs_after_control_delete(para, &self.styles, self.dpi);
        }
        if let Some(sec) = self.document.sections.get_mut(section_idx) {
            sec.raw_stream = None;
        }
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        Ok("{\"ok\":true}".to_string())
    }
}

fn cell_paragraph_mut(
    document: &mut crate::model::document::Document,
    section_idx: usize,
    parent_para_idx: usize,
    table_control_idx: usize,
    cell_idx: usize,
    cell_para_idx: usize,
) -> Result<&mut Paragraph, HwpError> {
    use crate::model::control::Control;
    let section = document
        .sections
        .get_mut(section_idx)
        .ok_or_else(|| HwpError::RenderError(format!("구역 인덱스 {section_idx} 범위 초과")))?;
    let para = section
        .paragraphs
        .get_mut(parent_para_idx)
        .ok_or_else(|| HwpError::RenderError(format!("부모 문단 인덱스 {parent_para_idx} 범위 초과")))?;
    let Some(Control::Table(table)) = para.controls.get_mut(table_control_idx) else {
        return Err(HwpError::RenderError(format!(
            "컨트롤 {table_control_idx} 는 표가 아닙니다"
        )));
    };
    let cell = table
        .cells
        .get_mut(cell_idx)
        .ok_or_else(|| HwpError::RenderError(format!("셀 인덱스 {cell_idx} 범위 초과")))?;
    cell.paragraphs
        .get_mut(cell_para_idx)
        .ok_or_else(|| HwpError::RenderError(format!("셀 문단 인덱스 {cell_para_idx} 범위 초과")))
}

/// 문단에서 컨트롤 하나를 빼고 그 컨트롤이 차지하던 8 유닛 갭을 닫는다.
///
/// `delete_picture_control_native` 의 갭 계산과 같다 — 텍스트 문자 사이의 8 유닛 공백을
/// 컨트롤 순서대로 대응시켜 `control_idx` 의 갭 시작을 찾고, 그 뒤 오프셋을 8 당긴다.
pub(crate) fn remove_control_from_paragraph(para: &mut Paragraph, control_idx: usize) {
    let text_chars: Vec<char> = para.text.chars().collect();
    let mut ci = 0usize;
    let mut prev_end: u32 = 0;
    let mut gap_start: Option<u32> = None;
    'outer: for (i, ch) in text_chars.iter().enumerate() {
        let offset = if i < para.char_offsets.len() {
            para.char_offsets[i]
        } else {
            prev_end
        };
        while prev_end + 8 <= offset && ci < para.controls.len() {
            if ci == control_idx {
                gap_start = Some(prev_end);
                break 'outer;
            }
            ci += 1;
            prev_end += 8;
        }
        let char_size: u32 = if *ch == '\t' {
            8
        } else if ch.len_utf16() == 2 {
            2
        } else {
            1
        };
        prev_end = offset + char_size;
    }
    if gap_start.is_none() {
        while ci < para.controls.len() {
            if ci == control_idx {
                gap_start = Some(prev_end);
                break;
            }
            ci += 1;
            prev_end += 8;
        }
    }
    if let Some(gs) = gap_start {
        let threshold = gs + 8;
        for offset in para.char_offsets.iter_mut() {
            if *offset >= threshold {
                *offset -= 8;
            }
        }
    }
    para.controls.remove(control_idx);
    if control_idx < para.ctrl_data_records.len() {
        para.ctrl_data_records.remove(control_idx);
    }
    for range in &mut para.field_ranges {
        if range.control_idx > control_idx {
            range.control_idx -= 1;
        }
    }
    if para.char_count >= 8 {
        para.char_count -= 8;
    }
}
