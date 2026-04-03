//! 从源码中读取方法前的 `/** ... */` Javadoc，作为接口说明（支持中文等任意语言）。

/// 取紧邻方法之前的 Javadoc 首段摘要（遇空行或 `@tag` 截止）。
///
/// 可能误把**类体之外**（类声明以上的）文档块当成方法注释；方法级应使用
/// `method_javadoc_summary_in_range`（传入 `class_body.start_byte()`）。
pub fn method_javadoc_summary(source: &str, method_start_byte: usize) -> Option<String> {
    let block = last_javadoc_block_before(source, method_start_byte)?;
    summarize_block(block)
}

/// 仅在 `class_body.start_byte() .. method.start_byte()` 内查找方法前 Javadoc（避免类头注释串入）。
pub fn method_javadoc_summary_in_range(
    source: &str,
    method_start_byte: usize,
    class_body_start_byte: usize,
) -> Option<String> {
    if method_start_byte <= class_body_start_byte {
        return None;
    }
    let slice = source.get(class_body_start_byte..method_start_byte)?;
    let block = last_javadoc_block_before(slice, slice.len())?;
    summarize_block(block)
}

fn summarize_block(block: &str) -> Option<String> {
    let lines = javadoc_lines(block);
    let summary = first_paragraph_from_lines(&lines);
    if summary.is_empty() {
        None
    } else {
        Some(summary)
    }
}

/// 在 `method_start_byte` 之前、结束位置最接近该字节的 `/** ... */` 块。
fn last_javadoc_block_before(source: &str, method_byte: usize) -> Option<&str> {
    let method_byte = method_byte.min(source.len());
    let prefix = source.get(..method_byte)?;
    let mut best: Option<(usize, usize)> = None; // (start, end exclusive end)

    let mut search_from = 0usize;
    while let Some(rel) = prefix[search_from..].find("/**") {
        let abs_start = search_from + rel;
        let after = abs_start + 3;
        if after >= prefix.len() {
            break;
        }
        if let Some(rel_e) = prefix[after..].find("*/") {
            let abs_end = after + rel_e + 2;
            if abs_end <= method_byte {
                match best {
                    None => best = Some((abs_start, abs_end)),
                    Some((_, prev_end)) if abs_end >= prev_end => {
                        best = Some((abs_start, abs_end));
                    }
                    _ => {}
                }
            }
            search_from = abs_start + 3;
        } else {
            break;
        }
    }

    let (s, e) = best?;
    prefix.get(s..e)
}

fn javadoc_lines(block: &str) -> Vec<String> {
    let inner = match block.trim().strip_prefix("/**") {
        Some(b) => b.split("*/").next().unwrap_or("").trim(),
        None => return Vec::new(),
    };
    inner
        .lines()
        .map(|line| line.trim().trim_start_matches('*').trim().to_string())
        .collect()
}

fn first_paragraph_from_lines(lines: &[String]) -> String {
    let mut acc: Vec<&str> = Vec::new();
    for l in lines {
        if l.is_empty() {
            break;
        }
        if l.starts_with('@') {
            break;
        }
        acc.push(l.as_str());
    }
    acc.join(" ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_nearest_block() {
        let src = r#"
class X {
    /** old */
    void a() {}
    /** 你好世界 second method */
    @GetMapping("/x")
    void b() {}
}
"#;
        let method_b = src.find("void b").unwrap();
        let s = method_javadoc_summary(src, method_b);
        assert_eq!(s.as_deref(), Some("你好世界 second method"));
    }

    #[test]
    fn in_range_ignores_class_javadoc_above_rest_controller() {
        let src = r#"
/** 付款单 */
@RestController
class FinPaymentBillController {
    @Operation(summary = "付款单列表")
    @PostMapping("/list")
    public void list() {}
}
"#;
        let list_pos = src.find("public void list").unwrap();
        let body_start = src.find('{').map(|i| i + 1).unwrap();
        assert_eq!(
            method_javadoc_summary_in_range(src, list_pos, body_start).as_deref(),
            None
        );
        assert_eq!(
            method_javadoc_summary(src, list_pos).as_deref(),
            Some("付款单")
        );
    }
}
