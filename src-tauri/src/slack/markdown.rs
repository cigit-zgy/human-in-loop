//! 把标准 Markdown 转换为 Slack `mrkdwn`。
//!
//! Slack mrkdwn 方言：粗体 `*x*`、斜体 `_x_`、删除线 `~x~`、行内码 `` `x` ``、代码块 ```` ```x``` ````、
//! 引用 `> x`、链接 `<url|text>`；特殊字符仅需转义 `& < >`。
//! 结构与 `telegram/markdown.rs` 同构（逐行识别块级 + 行内扫描），但输出 mrkdwn 而非 HTML。

/// 转义 mrkdwn 文本中的特殊字符（`&` 必须最先处理）。
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 块级转换入口：逐行识别代码块/表格/引用/标题/列表，其余按段落做行内转换。
pub fn to_mrkdwn(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // 围栏代码块 ``` / ```lang（mrkdwn 无语言标记，丢弃 lang）。
        if trimmed.starts_with("```") {
            let mut body: Vec<String> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                body.push(lines[i].to_string());
                i += 1;
            }
            if i < lines.len() {
                i += 1; // 跳过收尾围栏
            }
            out.push(format!("```\n{}\n```", escape(&body.join("\n"))));
            continue;
        }

        // GFM 表格：当前行含 `|` 且下一行是分隔行 → 渲染为等宽代码块。
        if line.contains('|') && i + 1 < lines.len() && is_table_separator(lines[i + 1]) {
            let mut rows: Vec<&str> = vec![lines[i]];
            let mut j = i + 2;
            while j < lines.len() && lines[j].contains('|') && !lines[j].trim().is_empty() {
                rows.push(lines[j]);
                j += 1;
            }
            out.push(render_table(&rows));
            i = j;
            continue;
        }

        // 引用块：连续 `>` 行各自前缀 `> `。
        if trimmed.starts_with('>') {
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let content = lines[i].trim_start().trim_start_matches('>');
                let content = content.strip_prefix(' ').unwrap_or(content);
                out.push(format!("> {}", inline(content)));
                i += 1;
            }
            continue;
        }

        // ATX 标题 → 加粗整行。
        if let Some(title) = header_line(line) {
            out.push(format!("*{}*", inline(&title)));
            i += 1;
            continue;
        }

        // 无序列表项 `- ` / `* ` / `+ ` → `• `（保留缩进以维持层级）。
        if let Some(item) = unordered_item(trimmed) {
            let indent_len = line.len() - trimmed.len();
            out.push(format!("{}• {}", &line[..indent_len], inline(item)));
            i += 1;
            continue;
        }

        // 普通段落行：行内转换（有序列表 `1.` 也走这里，序号原样保留）。
        out.push(inline(line));
        i += 1;
    }
    out.join("\n")
}

/// 行内转换：行内码 / 链接 / 粗体(`**`,`__`) / 删除线(`~~`) / 斜体(`*`,`_`)，其余字符做转义。
fn inline(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        // 行内码：`code`（内部不再做 markdown，仅转义）。
        if c == '`' {
            if let Some(close) = find_char(&chars, i + 1, '`') {
                let content: String = chars[i + 1..close].iter().collect();
                out.push('`');
                out.push_str(&escape(&content));
                out.push('`');
                i = close + 1;
                continue;
            }
        }

        // 链接 [text](url) → <url|text>（label 仅转义，不再做行内 markdown）。
        if c == '[' {
            if let Some((text, url, next)) = parse_link(&chars, i) {
                out.push_str(&format!("<{}|{}>", url.trim(), escape(&text)));
                i = next;
                continue;
            }
        }

        // 粗体 ** 或 __ → *...*。
        if (c == '*' || c == '_') && i + 1 < chars.len() && chars[i + 1] == c {
            if let Some(close) = find_double(&chars, i + 2, c) {
                let content: String = chars[i + 2..close].iter().collect();
                out.push('*');
                out.push_str(&inline(&content));
                out.push('*');
                i = close + 2;
                continue;
            }
        }

        // 删除线 ~~ → ~...~。
        if c == '~' && i + 1 < chars.len() && chars[i + 1] == '~' {
            if let Some(close) = find_double(&chars, i + 2, '~') {
                let content: String = chars[i + 2..close].iter().collect();
                out.push('~');
                out.push_str(&inline(&content));
                out.push('~');
                i = close + 2;
                continue;
            }
        }

        // 斜体 * 或 _ → _..._（带边界判断，避免吃掉 snake_case 等）。
        if c == '*' || c == '_' {
            if let Some(close) = find_italic_close(&chars, i, c) {
                let content: String = chars[i + 1..close].iter().collect();
                out.push('_');
                out.push_str(&inline(&content));
                out.push('_');
                i = close + 1;
                continue;
            }
        }

        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

fn find_char(chars: &[char], start: usize, ch: char) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == ch)
}

/// 自 `start` 起找到首个成对标记 `chch` 的起始下标；内容非空才算。
fn find_double(chars: &[char], start: usize, ch: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == ch && chars[i + 1] == ch {
            return if i > start { Some(i) } else { None };
        }
        i += 1;
    }
    None
}

/// 斜体闭合：跳过成对标记，要求内容非空、首尾非空白；`_` 额外要求词边界。
fn find_italic_close(chars: &[char], open: usize, ch: char) -> Option<usize> {
    if ch == '_' && open > 0 && chars[open - 1].is_alphanumeric() {
        return None;
    }
    if open + 1 >= chars.len() || chars[open + 1].is_whitespace() {
        return None;
    }
    let mut j = open + 1;
    while j < chars.len() {
        let cj = chars[j];
        if cj == '`' {
            break; // 不跨越行内码
        }
        if cj == ch {
            if j + 1 < chars.len() && chars[j + 1] == ch {
                j += 2; // 跳过成对标记
                continue;
            }
            if j > open + 1 && !chars[j - 1].is_whitespace() {
                if ch == '_' && j + 1 < chars.len() && chars[j + 1].is_alphanumeric() {
                    j += 1;
                    continue;
                }
                return Some(j);
            }
        }
        j += 1;
    }
    None
}

fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close_br = find_char(chars, start + 1, ']')?;
    if close_br + 1 >= chars.len() || chars[close_br + 1] != '(' {
        return None;
    }
    let close_par = find_char(chars, close_br + 2, ')')?;
    let text: String = chars[start + 1..close_br].iter().collect();
    let url: String = chars[close_br + 2..close_par].iter().collect();
    if url.trim().is_empty() {
        return None;
    }
    Some((text, url, close_par + 1))
}

/// `^#{1,6}\s+(.+)$` → 返回标题文本。
fn header_line(line: &str) -> Option<String> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &line[hashes..];
        let title = rest.trim_start_matches([' ', '\t']);
        if title.len() < rest.len() && !title.is_empty() {
            return Some(title.to_string());
        }
    }
    None
}

fn unordered_item(trimmed: &str) -> Option<&str> {
    for p in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(p) {
            return Some(rest);
        }
    }
    None
}

/// GFM 分隔行：去掉首尾 `|` 后，每个单元格仅由 `-`/`:` 组成且含 `-`。
fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('|') {
        return false;
    }
    let core = t.trim_start_matches('|').trim_end_matches('|');
    let cells: Vec<&str> = core.split('|').collect();
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|c| {
        let c = c.trim();
        !c.is_empty() && c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':')
    })
}

fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// 表格 → 等宽代码块（按列宽对齐，表头下加一行分隔线）。
fn render_table(rows: &[&str]) -> String {
    let parsed: Vec<Vec<String>> = rows.iter().map(|r| split_row(r)).collect();
    let cols = parsed.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return String::new();
    }
    let mut widths = vec![0usize; cols];
    for r in &parsed {
        for (ci, c) in r.iter().enumerate() {
            widths[ci] = widths[ci].max(c.chars().count());
        }
    }
    let mut lines: Vec<String> = Vec::new();
    for (ri, r) in parsed.iter().enumerate() {
        let cells: Vec<String> = (0..cols)
            .map(|ci| {
                let cell = r.get(ci).cloned().unwrap_or_default();
                let pad = widths[ci].saturating_sub(cell.chars().count());
                format!("{}{}", cell, " ".repeat(pad))
            })
            .collect();
        lines.push(cells.join(" | "));
        if ri == 0 {
            let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
            lines.push(sep.join("-+-"));
        }
    }
    format!("```\n{}\n```", escape(&lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_italic_strike() {
        assert_eq!(to_mrkdwn("**b**"), "*b*");
        assert_eq!(to_mrkdwn("__b__"), "*b*");
        assert_eq!(to_mrkdwn("*i*"), "_i_");
        assert_eq!(to_mrkdwn("_i_"), "_i_");
        assert_eq!(to_mrkdwn("~~s~~"), "~s~");
    }

    #[test]
    fn snake_case_not_italic() {
        assert_eq!(to_mrkdwn("file_name_here"), "file_name_here");
    }

    #[test]
    fn header_bold() {
        assert_eq!(to_mrkdwn("# Title"), "*Title*");
        assert_eq!(to_mrkdwn("### Sub"), "*Sub*");
    }

    #[test]
    fn special_chars_escaped() {
        assert_eq!(to_mrkdwn("a < b & c > d"), "a &lt; b &amp; c &gt; d");
    }

    #[test]
    fn inline_code_escaped() {
        assert_eq!(to_mrkdwn("`a<b>`"), "`a&lt;b&gt;`");
    }

    #[test]
    fn code_block() {
        assert_eq!(to_mrkdwn("```\na<b\n```"), "```\na&lt;b\n```");
    }

    #[test]
    fn code_block_lang_dropped() {
        assert_eq!(to_mrkdwn("```rust\nlet x=1;\n```"), "```\nlet x=1;\n```");
    }

    #[test]
    fn unordered_list_bullets() {
        assert_eq!(to_mrkdwn("- a\n- b"), "• a\n• b");
    }

    #[test]
    fn link() {
        assert_eq!(to_mrkdwn("[t](https://x.com)"), "<https://x.com|t>");
    }

    #[test]
    fn blockquote() {
        assert_eq!(to_mrkdwn("> hi\n> there"), "> hi\n> there");
    }

    #[test]
    fn nested_bold_italic() {
        assert_eq!(to_mrkdwn("**a *b* c**"), "*a _b_ c*");
    }
}
