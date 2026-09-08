//! 把标准 Markdown 转换为 Telegram HTML（`parse_mode=HTML`）。
//!
//! 选用 HTML 而非 MarkdownV2：HTML 只需转义 `< > &` 三个字符，标签天然配对，
//! 几乎不会因某个特殊字符不配对导致整条消息解析失败回退纯文本；同时能稳定覆盖
//! 粗体/斜体/删除线/行内码/代码块/引用/链接，并把表格转等宽代码块、列表美化为 `• `。
//!
//! 支持的标签：`<b> <i> <s> <code> <pre> <blockquote> <a>`（Telegram HTML 子集）。

/// 转义 HTML 文本节点中的特殊字符（`&` 必须最先处理）。
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 转义放入属性（如 `href`）的文本。
fn escape_attr(text: &str) -> String {
    escape_html(text).replace('"', "&quot;")
}

/// 块级转换入口：逐行识别代码块/表格/引用/标题/列表，其余按段落做行内转换。
pub fn to_html(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // 围栏代码块 ``` / ```lang。
        if let Some(lang) = trimmed.strip_prefix("```") {
            let lang = lang.trim().to_string();
            let mut body: Vec<String> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                body.push(lines[i].to_string());
                i += 1;
            }
            if i < lines.len() {
                i += 1; // 跳过收尾围栏
            }
            let escaped = escape_html(&body.join("\n"));
            if lang.is_empty() {
                out.push(format!("<pre>{}</pre>", escaped));
            } else {
                out.push(format!(
                    "<pre><code class=\"language-{}\">{}</code></pre>",
                    escape_html(&lang),
                    escaped
                ));
            }
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

        // 引用块：连续 `>` 行合并为一个 <blockquote>。
        if trimmed.starts_with('>') {
            let mut quote: Vec<String> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let content = lines[i].trim_start().trim_start_matches('>');
                let content = content.strip_prefix(' ').unwrap_or(content);
                quote.push(inline(content));
                i += 1;
            }
            out.push(format!("<blockquote>{}</blockquote>", quote.join("\n")));
            continue;
        }

        // ATX 标题 → 加粗整行。
        if let Some(title) = header_line(line) {
            out.push(format!("<b>{}</b>", inline(&title)));
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

/// 行内转换：行内码 / 链接 / 粗体(`**`,`__`) / 删除线(`~~`) / 斜体(`*`,`_`)，其余字符做 HTML 转义。
fn inline(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        // 行内码：`code`（内部不再做 markdown，仅 HTML 转义）。
        if c == '`' {
            if let Some(close) = find_char(&chars, i + 1, '`') {
                let content: String = chars[i + 1..close].iter().collect();
                out.push_str("<code>");
                out.push_str(&escape_html(&content));
                out.push_str("</code>");
                i = close + 1;
                continue;
            }
        }

        // 链接 [text](url)。
        if c == '[' {
            if let Some((text, url, next)) = parse_link(&chars, i) {
                out.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    escape_attr(&url),
                    inline(&text)
                ));
                i = next;
                continue;
            }
        }

        // 粗体 ** 或 __。
        if (c == '*' || c == '_') && i + 1 < chars.len() && chars[i + 1] == c {
            if let Some(close) = find_double(&chars, i + 2, c) {
                let content: String = chars[i + 2..close].iter().collect();
                out.push_str("<b>");
                out.push_str(&inline(&content));
                out.push_str("</b>");
                i = close + 2;
                continue;
            }
        }

        // 删除线 ~~。
        if c == '~' && i + 1 < chars.len() && chars[i + 1] == '~' {
            if let Some(close) = find_double(&chars, i + 2, '~') {
                let content: String = chars[i + 2..close].iter().collect();
                out.push_str("<s>");
                out.push_str(&inline(&content));
                out.push_str("</s>");
                i = close + 2;
                continue;
            }
        }

        // 斜体 * 或 _（带边界判断，避免吃掉 snake_case 等）。
        if c == '*' || c == '_' {
            if let Some(close) = find_italic_close(&chars, i, c) {
                let content: String = chars[i + 1..close].iter().collect();
                out.push_str("<i>");
                out.push_str(&inline(&content));
                out.push_str("</i>");
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

/// 自 `start` 起找到首个成对标记 `chch` 的起始下标；内容非空才算（`start` 处即命中视为空内容）。
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

/// 斜体闭合：跳过成对标记（粗体/删除线），要求内容非空、首尾非空白；`_` 额外要求词边界。
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
    format!("<pre>{}</pre>", escape_html(&lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_italic_strike() {
        assert_eq!(to_html("**b**"), "<b>b</b>");
        assert_eq!(to_html("*i*"), "<i>i</i>");
        assert_eq!(to_html("_i_"), "<i>i</i>");
        assert_eq!(to_html("~~s~~"), "<s>s</s>");
    }

    #[test]
    fn snake_case_not_italic() {
        assert_eq!(to_html("file_name_here"), "file_name_here");
    }

    #[test]
    fn header_bold() {
        assert_eq!(to_html("# Title"), "<b>Title</b>");
        assert_eq!(to_html("### Sub"), "<b>Sub</b>");
    }

    #[test]
    fn html_chars_escaped() {
        assert_eq!(to_html("a < b & c > d"), "a &lt; b &amp; c &gt; d");
    }

    #[test]
    fn inline_code_escaped() {
        assert_eq!(to_html("`a<b>`"), "<code>a&lt;b&gt;</code>");
    }

    #[test]
    fn code_block() {
        assert_eq!(to_html("```\na<b\n```"), "<pre>a&lt;b</pre>");
    }

    #[test]
    fn code_block_lang() {
        assert_eq!(
            to_html("```rust\nlet x=1;\n```"),
            "<pre><code class=\"language-rust\">let x=1;</code></pre>"
        );
    }

    #[test]
    fn unordered_list_bullets() {
        assert_eq!(to_html("- a\n- b"), "• a\n• b");
    }

    #[test]
    fn link() {
        assert_eq!(
            to_html("[t](https://x.com)"),
            "<a href=\"https://x.com\">t</a>"
        );
    }

    #[test]
    fn blockquote() {
        assert_eq!(
            to_html("> hi\n> there"),
            "<blockquote>hi\nthere</blockquote>"
        );
    }

    #[test]
    fn table_to_pre() {
        let got = to_html("| a | bb |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(got, "<pre>a | bb\n--+---\n1 | 2 </pre>");
    }

    #[test]
    fn nested_bold_italic() {
        assert_eq!(to_html("**a *b* c**"), "<b>a <i>b</i> c</b>");
    }
}
