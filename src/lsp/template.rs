use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, Position, Range, TextEdit,
};
use minijinja::Environment;

// 声明模板上下文中可静态识别的对象字段。
const PLAN_FIELDS: &[(&str, &str)] = &[
    ("root_target", "用户指定的最终目标的限定名称"),
    ("stages", "依赖在前的编译阶段列表"),
];
const STAGE_FIELDS: &[(&str, &str)] = &[
    ("index", "从 1 开始的阶段编号"),
    ("targets", "本阶段可并行实施的目标列表"),
];
const TARGET_FIELDS: &[(&str, &str)] = &[
    ("id", "目标的限定名称"),
    ("name", "目标的本地名称"),
    ("dependencies", "直接前置目标的限定名称列表"),
    ("description", "原始目标描述"),
    ("specifications", "原始验收规格列表"),
];

// 判断文件是否为 mkd 的 Markdown 模板，而非其他项目的通用 Jinja 文件。
pub(super) fn is_template_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".md.j2"))
}

// 仅编译模板并检查字面 plan 字段；不绑定目标，也不执行模板。
pub(super) fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut environment = Environment::new();
    if let Err(error) = environment.add_template("plan.md", source) {
        let range = error
            .range()
            .and_then(|span| source.get(..span.start).map(|_| span))
            .map(|span| span_range(source, span.start, span.end))
            .unwrap_or_else(|| {
                let line = error.line().unwrap_or(1).saturating_sub(1);
                let text = source.lines().nth(line).unwrap_or("");
                Range::new(
                    Position::new(u32::try_from(line).unwrap_or(u32::MAX), 0),
                    Position::new(
                        u32::try_from(line).unwrap_or(u32::MAX),
                        u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX),
                    ),
                )
            });
        return vec![Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(lsp_types::NumberOrString::String("T001".into())),
            source: Some("mkd".into()),
            message: error.to_string(),
            ..Default::default()
        }];
    }

    let mut found = Vec::new();
    scan_tags(source, |tag, offset| {
        // 字符串和注释不应被当作上下文变量；只识别直接的 plan.<名称>。
        let bytes = tag.as_bytes();
        let mut index = 0;
        let mut quote = None;
        while index < bytes.len() {
            if let Some(delimiter) = quote {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if bytes[index] == delimiter {
                    quote = None;
                }
                index += 1;
                continue;
            }
            if matches!(bytes[index], b'\'' | b'"') {
                quote = Some(bytes[index]);
                index += 1;
                continue;
            }
            if bytes[index..].starts_with(b"plan.")
                && (index == 0 || !is_identifier_byte(bytes[index - 1]) && bytes[index - 1] != b'.')
            {
                let start = index + 5;
                let mut end = start;
                while end < bytes.len() && is_identifier_byte(bytes[end]) {
                    end += 1;
                }
                let field = &tag[start..end];
                if !field.is_empty() && !PLAN_FIELDS.iter().any(|(name, _)| *name == field) {
                    found.push(Diagnostic {
                        range: span_range(source, offset + start, offset + end),
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(lsp_types::NumberOrString::String("T002".into())),
                        source: Some("mkd".into()),
                        message: format!(
                            "未知的 plan 字段 `{field}`；可用字段：root_target、stages"
                        ),
                        ..Default::default()
                    });
                }
                index = end.max(index + 1);
            } else {
                index += 1;
            }
        }
    });
    found
}

// 遍历插值和控制标签，跳过注释及 raw 块。
fn scan_tags(source: &str, mut inspect: impl FnMut(&str, usize)) {
    let mut cursor = 0;
    while cursor < source.len() {
        let Some(next) = ["{#", "{%", "{{"]
            .into_iter()
            .filter_map(|tag| {
                source[cursor..]
                    .find(tag)
                    .map(|index| (cursor + index, tag))
            })
            .min_by_key(|(index, _)| *index)
        else {
            break;
        };
        let (start, opening) = next;
        let closing = match opening {
            "{#" => "#}",
            "{%" => "%}",
            _ => "}}",
        };
        let mut end = start + 2;
        let mut quote = None;
        while end < source.len() {
            let byte = source.as_bytes()[end];
            if opening != "{#" {
                if let Some(delimiter) = quote {
                    if byte == b'\\' {
                        end = (end + 2).min(source.len());
                        continue;
                    }
                    if byte == delimiter {
                        quote = None;
                    }
                    end += 1;
                    continue;
                }
                if matches!(byte, b'\'' | b'"') {
                    quote = Some(byte);
                    end += 1;
                    continue;
                }
            }
            if source.as_bytes()[end..].starts_with(closing.as_bytes()) {
                break;
            }
            end += 1;
        }
        if end == source.len() {
            break;
        }
        let body = &source[start + 2..end];
        if opening == "{#" {
            cursor = end + 2;
            continue;
        }
        if closing == "%}" && body.trim_matches(|c: char| c.is_whitespace() || c == '-') == "raw" {
            let mut lookahead = end + 2;
            while let Some(relative) = source[lookahead..].find("{%") {
                let start = lookahead + relative + 2;
                let Some(stop) = source[start..].find("%}") else {
                    break;
                };
                let stop = start + stop;
                if source[start..stop].trim_matches(|c: char| c.is_whitespace() || c == '-')
                    == "endraw"
                {
                    cursor = stop + 2;
                    break;
                }
                lookahead = stop + 2;
            }
            if cursor > end + 2 {
                continue;
            }
        }
        inspect(body, start + 2);
        cursor = end + 2;
    }
}

// 判断 ASCII 字节是否可组成字段名的一部分。
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

// 将 UTF-8 字节偏移转换为 LSP 使用的零基 UTF-16 位置。
fn position_at(source: &str, offset: usize) -> Position {
    let prefix = source.get(..offset).unwrap_or("");
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count();
    Position::new(
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(column).unwrap_or(u32::MAX),
    )
}

// 将语法片段在源码中的字节区间映射为 LSP 范围。
fn span_range(source: &str, start: usize, end: usize) -> Range {
    Range::new(position_at(source, start), position_at(source, end))
}

// 按当前插值或控制标签中的对象前缀提供只读字段补全。
pub(super) fn completions(source: &str, position: Position) -> Vec<CompletionItem> {
    let Some(line) = source.split('\n').nth(position.line as usize) else {
        return Vec::new();
    };
    let mut units = 0;
    let mut offset = None;
    for (index, ch) in line.char_indices() {
        if units == position.character as usize {
            offset = Some(index);
            break;
        }
        units += ch.len_utf16();
    }
    let offset = offset.or_else(|| (units == position.character as usize).then_some(line.len()));
    let Some(offset) = offset else {
        return Vec::new();
    };
    let before = &line[..offset];
    let last_comment_start = before.rfind("{#");
    let last_comment_end = before.rfind("#}");
    if last_comment_start.is_some_and(|start| last_comment_end.is_none_or(|end| start > end)) {
        return Vec::new();
    }
    let opening = [before.rfind("{{"), before.rfind("{%")]
        .into_iter()
        .flatten()
        .max();
    let Some(opening) = opening else {
        return Vec::new();
    };
    let mut quote = None;
    for byte in before.as_bytes()[opening + 2..].iter().copied() {
        match (quote, byte) {
            (Some(delimiter), current) if delimiter == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            _ => {}
        }
    }
    if quote.is_some() {
        return Vec::new();
    }
    if before[opening..].contains("}}")
        || before[opening..].contains("%}")
        || before[opening..].contains("{#")
    {
        return Vec::new();
    }
    let mut word_start = offset;
    while word_start > 0 && is_identifier_byte(line.as_bytes()[word_start - 1]) {
        word_start -= 1;
    }
    if word_start == 0 || line.as_bytes()[word_start - 1] != b'.' {
        return Vec::new();
    }
    let name_end = word_start - 1;
    let mut name_start = name_end;
    while name_start > 0 && is_identifier_byte(line.as_bytes()[name_start - 1]) {
        name_start -= 1;
    }
    if name_start == name_end || name_start < opening + 2 {
        return Vec::new();
    }
    if name_start > 0
        && (is_identifier_byte(line.as_bytes()[name_start - 1])
            || line.as_bytes()[name_start - 1] == b'.')
    {
        return Vec::new();
    }
    let fields = match &line[name_start..name_end] {
        "plan" => PLAN_FIELDS,
        "stage" => STAGE_FIELDS,
        "target" => TARGET_FIELDS,
        _ => return Vec::new(),
    };
    let prefix = &line[word_start..offset];
    let mut word_end = offset;
    while word_end < line.len() && is_identifier_byte(line.as_bytes()[word_end]) {
        word_end += 1;
    }
    let start = position.character - u32::try_from(prefix.encode_utf16().count()).unwrap_or(0);
    let end = position.character
        + u32::try_from(line[offset..word_end].encode_utf16().count()).unwrap_or(0);
    fields
        .iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(name, detail)| CompletionItem {
            label: (*name).into(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some((*detail).into()),
            text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit {
                range: Range::new(
                    Position::new(position.line, start),
                    Position::new(position.line, end),
                ),
                new_text: (*name).into(),
            })),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证未知字段按 Unicode 字符位置报告且注释和字符串不误报。
    #[test]
    fn template_diagnostics_check_syntax_and_literal_plan_fields() {
        let source = "😀 {{ 'plan.fake' }} {{ plan.stegs }}\n{# plan.wrong #}\n{% for stage in plan.stages %}{{ stage.index }}{% endfor %}";
        let errors = diagnostics(source);
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].code,
            Some(lsp_types::NumberOrString::String("T002".into()))
        );
        assert_eq!(errors[0].range.start, Position::new(0, 29));
        assert_eq!(
            errors[0].message,
            "未知的 plan 字段 `stegs`；可用字段：root_target、stages"
        );
        let syntax = diagnostics("{% for stage in plan.stages %}");
        assert_eq!(syntax.len(), 1);
        assert_eq!(
            syntax[0].code,
            Some(lsp_types::NumberOrString::String("T001".into()))
        );
    }

    // 验证扫描器不会误诊注释、raw 块、字符串或尚未绑定的局部变量。
    #[test]
    fn template_diagnostics_skip_non_expressions() {
        let source = "{# plan.fake #}\n{%- raw -%}{{ plan.bad }}{%- endraw -%}\n{{ 'plan.ghost' }}\n{% set other = plan.root_target %}\n{{ other.value }}";
        assert!(diagnostics(source).is_empty());
    }

    // 验证仓库内全部中文模板不会产生误报。
    #[test]
    fn example_templates_pass_static_validation() {
        for source in [
            include_str!("../../examples/templates/01-单目标简报.md.j2"),
            include_str!("../../examples/templates/02-写作任务清单.md.j2"),
            include_str!("../../examples/templates/03-并行发布看板.md.j2"),
            include_str!("../../examples/templates/04-质量门禁验收.md.j2"),
            include_str!("../../examples/templates/05-流水线交接手册.md.j2"),
        ] {
            assert!(diagnostics(source).is_empty(), "{source}");
        }
    }

    // 验证属性补全限定在模板标签中且按已输入的前缀缩小候选。
    #[test]
    fn template_completions_are_contextual() {
        let source = "{{ plan.ro }}\n{% for stage in plan.stages %}{{ stage. }}{% endfor %}\n普通文字 target.";
        let items = completions(source, Position::new(0, 10));
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["root_target"]
        );
        let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &items[0].text_edit else {
            panic!("字段补全必须包含编辑范围");
        };
        assert_eq!(edit.range.start.character, 8);
        let middle = completions("{{ plan.root_target }}", Position::new(0, 10));
        let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &middle[0].text_edit else {
            panic!("字段补全必须包含编辑范围");
        };
        assert_eq!(edit.range.end.character, 19);
        assert!(completions(source, Position::new(2, 11)).is_empty());
        assert!(completions("{# {{ plan.ro #}", Position::new(0, 13)).is_empty());
        assert!(completions("{{ 'plan.ro' }}", Position::new(0, 11)).is_empty());
        assert_eq!(
            completions("{{ target.spe }}", Position::new(0, 13))
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["specifications"]
        );
    }
}
