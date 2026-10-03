//! Transient, toolkit-free selected-file changes against HEAD.

use serde::{Deserialize, Serialize};

pub const MAX_PREVIEW_BYTES: usize = 512 * 1024;
pub const MAX_PREVIEW_LINES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub untracked: bool,
    pub content: DiffContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffContent {
    Text {
        hunks: Vec<DiffHunk>,
        truncated: bool,
        notice: Option<String>,
    },
    Binary,
    Unchanged,
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u64,
    pub old_count: u64,
    pub new_start: u64,
    pub new_count: u64,
    pub heading: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    NoNewline,
}

/// Parse one Git unified patch. Filename headers are never interpreted.
pub fn parse_patch(bytes: &[u8], mut truncated: bool) -> Result<DiffContent, String> {
    if bytes.contains(&0) {
        return Ok(DiffContent::Binary);
    }
    let bytes = if bytes.len() > MAX_PREVIEW_BYTES {
        truncated = true;
        &bytes[..MAX_PREVIEW_BYTES]
    } else {
        bytes
    };
    let bytes = if truncated && !bytes.ends_with(b"\n") {
        &bytes[..bytes.iter().rposition(|b| *b == b'\n').map_or(0, |p| p + 1)]
    } else {
        bytes
    };
    let text = std::str::from_utf8(bytes).map_err(|_| "Text is not valid UTF-8".to_string())?;
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let (mut old_used, mut new_used) = (0u64, 0u64);
    let mut in_hunk = false;
    let mut lines = 0usize;
    for line in text.split_terminator('\n') {
        if line.starts_with("@@ ") {
            if let Some(previous) = hunks.last() {
                if old_used != previous.old_count || new_used != previous.new_count {
                    return Err("Incomplete hunk before next heading".into());
                }
            }
            let mut parts = line.splitn(4, ' ');
            parts.next();
            let (old_start, old_count) = range(parts.next(), '-')?;
            let (new_start, new_count) = range(parts.next(), '+')?;
            if !parts.next().is_some_and(|tail| tail.starts_with("@@")) {
                return Err("Invalid hunk heading".into());
            }
            old_start
                .checked_add(old_count)
                .ok_or("Hunk range overflow")?;
            new_start
                .checked_add(new_count)
                .ok_or("Hunk range overflow")?;
            hunks.push(DiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                heading: line.into(),
                lines: Vec::new(),
            });
            old_used = 0;
            new_used = 0;
            in_hunk = true;
            continue;
        }
        if line.starts_with("diff --git ") {
            if hunks
                .last()
                .is_some_and(|h| old_used != h.old_count || new_used != h.new_count)
            {
                return Err("Incomplete hunk before next patch".into());
            }
            in_hunk = false;
        }
        if !in_hunk {
            if line.starts_with("Binary files ") || line == "GIT binary patch" {
                return Ok(DiffContent::Binary);
            }
            continue;
        }
        let hunk = hunks.last_mut().unwrap();
        if lines == MAX_PREVIEW_LINES {
            truncated = true;
            break;
        }
        let (kind, old_line, new_line) = match line.as_bytes().first() {
            Some(b' ') => (
                DiffLineKind::Context,
                Some(hunk.old_start + old_used),
                Some(hunk.new_start + new_used),
            ),
            Some(b'-') => (DiffLineKind::Removed, Some(hunk.old_start + old_used), None),
            Some(b'+') => (DiffLineKind::Added, None, Some(hunk.new_start + new_used)),
            Some(b'\\') if line == "\\ No newline at end of file" => {
                (DiffLineKind::NoNewline, None, None)
            }
            _ => return Err("Invalid unified diff line".into()),
        };
        old_used += u64::from(old_line.is_some());
        new_used += u64::from(new_line.is_some());
        if old_used > hunk.old_count || new_used > hunk.new_count {
            return Err("Diff line exceeds hunk range".into());
        }
        hunk.lines.push(DiffLine {
            kind,
            text: if kind == DiffLineKind::NoNewline {
                line[2..].into()
            } else {
                line[1..].into()
            },
            old_line,
            new_line,
        });
        lines += 1;
    }
    if !truncated
        && hunks
            .last()
            .is_some_and(|h| old_used != h.old_count || new_used != h.new_count)
    {
        return Err("Incomplete final hunk".into());
    }
    let notice = if truncated {
        Some("Preview truncated at the size or line limit.".into())
    } else if hunks.is_empty() {
        Some("No text changes; file metadata changed.".into())
    } else {
        None
    };
    Ok(DiffContent::Text {
        hunks,
        truncated,
        notice,
    })
}

fn range(value: Option<&str>, prefix: char) -> Result<(u64, u64), String> {
    let value = value
        .and_then(|v| v.strip_prefix(prefix))
        .ok_or("Invalid hunk range")?;
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    Ok((
        start.parse().map_err(|_| "Invalid hunk start")?,
        count.parse().map_err(|_| "Invalid hunk count")?,
    ))
}

/// New regular UTF-8 text is one addition hunk against an empty file.
pub fn untracked_text(text: &str) -> DiffContent {
    let count = text.split_terminator('\n').count();
    if count == 0 {
        return DiffContent::Text {
            hunks: Vec::new(),
            truncated: false,
            notice: Some("Empty untracked file.".into()),
        };
    }
    let mut lines: Vec<_> = text
        .split_terminator('\n')
        .take(MAX_PREVIEW_LINES)
        .enumerate()
        .map(|(index, text)| DiffLine {
            kind: DiffLineKind::Added,
            text: text.into(),
            old_line: None,
            new_line: Some(index as u64 + 1),
        })
        .collect();
    let mut truncated = count > MAX_PREVIEW_LINES;
    if !text.ends_with('\n') && !truncated {
        if lines.len() == MAX_PREVIEW_LINES {
            truncated = true;
        } else {
            lines.push(DiffLine {
                kind: DiffLineKind::NoNewline,
                text: "No newline at end of file".into(),
                old_line: None,
                new_line: None,
            });
        }
    }
    DiffContent::Text {
        hunks: vec![DiffHunk {
            old_start: 0,
            old_count: 0,
            new_start: 1,
            new_count: count as u64,
            heading: format!("@@ -0,0 +1,{count} @@"),
            lines,
        }],
        truncated,
        notice: truncated.then(|| "Preview truncated at the line limit.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_numbers_hunks_and_missing_newline_without_reading_filename_headers() {
        let patch = b"diff --git a/odd b/odd\n--- a/odd\n+++ b/odd\n@@ -1,2 +1,3 @@ heading\n same\r\n-old\n+new\n+extra\n\\ No newline at end of file\n@@ -20 +21 @@\n-before\n+after\n";
        let DiffContent::Text {
            hunks, truncated, ..
        } = parse_patch(patch, false).unwrap()
        else {
            panic!("text")
        };
        assert!(!truncated);
        assert_eq!(hunks.len(), 2);
        assert_eq!(
            hunks[0].lines[0],
            DiffLine {
                kind: DiffLineKind::Context,
                text: "same\r".into(),
                old_line: Some(1),
                new_line: Some(1)
            }
        );
        assert_eq!(hunks[0].lines[1].old_line, Some(2));
        assert_eq!(hunks[0].lines[2].new_line, Some(2));
        assert_eq!(hunks[0].lines[3].new_line, Some(3));
        assert_eq!(hunks[0].lines[4].kind, DiffLineKind::NoNewline);
        assert_eq!(hunks[0].lines[4].old_line, None);
        assert_eq!(hunks[1].lines[0].old_line, Some(20));
        assert_eq!(hunks[1].lines[1].new_line, Some(21));
    }

    #[test]
    fn parser_reads_two_patch_sections_for_a_regular_file_becoming_a_symlink() {
        let patch = b"diff --git a/file b/file\ndeleted file mode 100644\n--- a/file\n+++ /dev/null\n@@ -1 +0,0 @@\n-content\ndiff --git a/file b/file\nnew file mode 120000\n--- /dev/null\n+++ b/file\n@@ -0,0 +1 @@\n+outside\n\\ No newline at end of file\n";
        let DiffContent::Text { hunks, .. } = parse_patch(patch, false).unwrap() else {
            panic!("text")
        };
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[1].lines[0].text, "outside");
        assert_eq!(hunks[1].lines[0].new_line, Some(1));
    }

    #[test]
    fn parser_limits_complete_lines_and_preserves_declared_ranges() {
        let patch = format!("@@ -0,0 +1,10001 @@\n{}", "+line\n".repeat(10_001));
        let DiffContent::Text {
            hunks,
            truncated,
            notice,
        } = parse_patch(patch.as_bytes(), false).unwrap()
        else {
            panic!("text")
        };
        assert!(truncated);
        assert!(notice.is_some());
        assert_eq!(hunks[0].new_count, 10_001);
        assert_eq!(hunks[0].lines.len(), MAX_PREVIEW_LINES);
        assert_eq!(hunks[0].lines.last().unwrap().new_line, Some(10_000));
        let DiffContent::Text {
            hunks, truncated, ..
        } = parse_patch(b"@@ -0,0 +1,2 @@\n+complete\n+partial", true).unwrap()
        else {
            panic!("text")
        };
        assert!(truncated);
        assert_eq!(hunks[0].lines.len(), 1);
        assert_eq!(hunks[0].lines[0].text, "complete");
    }

    #[test]
    fn parser_rejects_invalid_ranges_counts_and_encoding() {
        for patch in [
            b"@@ -1 +1 @@\n+extra\n+extra\n".as_slice(),
            b"@@ -2,2 +2,2 @@\n same\n",
            b"@@ -18446744073709551615,2 +1 @@\n",
            b"@@ -x +1 @@\n",
            b"@@ -1 +1 @@\n-\xff\n+x\n",
        ] {
            assert!(parse_patch(patch, false).is_err(), "{patch:?}");
        }
        assert_eq!(
            parse_patch(b"Binary files a/bin and b/bin differ\n", false).unwrap(),
            DiffContent::Binary
        );
    }

    #[test]
    fn nul_content_is_binary_even_when_git_forces_text_output() {
        assert_eq!(
            parse_patch(b"@@ -1 +1 @@\n-old\n+new\0content\n", false).unwrap(),
            DiffContent::Binary
        );
    }

    #[test]
    fn new_text_is_additions_with_newline_and_empty_states() {
        let DiffContent::Text {
            hunks, truncated, ..
        } = untracked_text("first\r\nlast")
        else {
            panic!("text")
        };
        assert!(!truncated);
        assert_eq!(hunks[0].old_count, 0);
        assert_eq!(hunks[0].new_count, 2);
        assert_eq!(hunks[0].lines[0].text, "first\r");
        assert_eq!(hunks[0].lines[1].new_line, Some(2));
        assert_eq!(hunks[0].lines[2].kind, DiffLineKind::NoNewline);
        let DiffContent::Text { hunks, notice, .. } = untracked_text("") else {
            panic!("text")
        };
        assert!(hunks.is_empty());
        assert!(notice.unwrap().contains("Empty"));
    }
}
