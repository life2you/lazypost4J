//! Body 多行编辑：字符光标位置（UTF-8 安全）与行/列换算。

/// `char_idx` 为字符序号（非字节）；`0` 表示第一个字符之前。
pub fn char_byte_index(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or_else(|| s.len())
}

/// 每行在「字符下标」上的半开区间 `[lo, hi)`（不含 `\n` 时 hi 为行末后一位）。
pub fn line_char_ranges(s: &str) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    for c in s.chars() {
        if c == '\n' {
            v.push((start, i));
            start = i + 1;
        }
        i += 1;
    }
    v.push((start, i));
    v
}

pub fn line_col_at(s: &str, cursor: usize) -> (usize, usize) {
    let ranges = line_char_ranges(s);
    for (li, &(lo, hi)) in ranges.iter().enumerate() {
        if cursor <= hi {
            return (li, cursor.saturating_sub(lo));
        }
    }
    if let Some(&(lo, hi)) = ranges.last() {
        return (
            ranges.len().saturating_sub(1),
            hi.saturating_sub(lo),
        );
    }
    (0, 0)
}

pub fn cursor_from_line_col(s: &str, line: usize, col: usize) -> usize {
    let ranges = line_char_ranges(s);
    if line >= ranges.len() {
        return s.chars().count();
    }
    let (lo, hi) = ranges[line];
    let width = hi.saturating_sub(lo);
    let c = col.min(width);
    lo + c
}

pub fn line_width_chars(s: &str, line: usize) -> usize {
    let ranges = line_char_ranges(s);
    ranges
        .get(line)
        .map(|(lo, hi)| hi.saturating_sub(*lo))
        .unwrap_or(0)
}

pub fn cursor_up(s: &str, cursor: usize) -> usize {
    let (line, col) = line_col_at(s, cursor);
    if line == 0 {
        return cursor;
    }
    let prev_w = line_width_chars(s, line - 1);
    let new_col = col.min(prev_w);
    cursor_from_line_col(s, line - 1, new_col)
}

pub fn cursor_down(s: &str, cursor: usize) -> usize {
    let ranges = line_char_ranges(s);
    let (line, col) = line_col_at(s, cursor);
    if line + 1 >= ranges.len() {
        return cursor;
    }
    let next_w = line_width_chars(s, line + 1);
    let new_col = col.min(next_w);
    cursor_from_line_col(s, line + 1, new_col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_ranges_and_up_down() {
        let s = "ab\ncd";
        assert_eq!(line_char_ranges(s), vec![(0, 2), (3, 5)]);
        assert_eq!(line_col_at(s, 0), (0, 0));
        assert_eq!(line_col_at(s, 2), (0, 2));
        assert_eq!(line_col_at(s, 3), (1, 0));
        assert_eq!(cursor_up(s, 3), 0);
        assert_eq!(cursor_down(s, 0), 3);
    }

    #[test]
    fn char_byte_index_utf8() {
        let s = "é";
        assert_eq!(char_byte_index(s, 0), 0);
        assert_eq!(char_byte_index(s, 1), 2);
        assert_eq!(char_byte_index(s, 2), 2);
    }
}
