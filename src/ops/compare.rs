//! Compare Files: what changed in the text between two versions of a PDF.

use anyhow::Result;
use similar::{ChangeTag, TextDiff};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Same,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub change: Change,
    /// 1-based page in the old file (removed/same) or new file (added).
    pub page: usize,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub lines: Vec<Line>,
    pub added: usize,
    pub removed: usize,
    pub old_pages: usize,
    pub new_pages: usize,
}

/// Line-by-line diff of the text of two documents (page texts given per page).
pub fn compare_texts(old_pages: &[String], new_pages: &[String]) -> Report {
    let (old, old_map) = flatten(old_pages);
    let (new, new_map) = flatten(new_pages);
    let diff = TextDiff::from_lines(&old, &new);
    let mut report = Report {
        old_pages: old_pages.len(),
        new_pages: new_pages.len(),
        ..Default::default()
    };
    for change in diff.iter_all_changes() {
        let text = change.value().trim_end_matches('\n').to_string();
        let (kind, page) = match change.tag() {
            ChangeTag::Equal => (Change::Same, old_map[change.old_index().unwrap_or(0)]),
            ChangeTag::Delete => {
                report.removed += 1;
                (Change::Removed, old_map[change.old_index().unwrap_or(0)])
            }
            ChangeTag::Insert => {
                report.added += 1;
                (Change::Added, new_map[change.new_index().unwrap_or(0)])
            }
        };
        report.lines.push(Line {
            change: kind,
            page,
            text,
        });
    }
    report
}

pub fn compare(
    old: &[u8],
    old_pw: Option<&str>,
    new: &[u8],
    new_pw: Option<&str>,
) -> Result<Report> {
    Ok(compare_texts(
        &super::page_texts(old, old_pw)?,
        &super::page_texts(new, new_pw)?,
    ))
}

/// Joins page texts into one newline-terminated string and maps each line to its page.
fn flatten(pages: &[String]) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut map = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        for line in page.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            text.push_str(line);
            text.push('\n');
            map.push(i + 1);
        }
    }
    if map.is_empty() {
        map.push(1);
    }
    (text, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_added_and_removed_lines_with_pages() {
        let old = vec!["Title\nPrice: 10".to_string(), "Footer".to_string()];
        let new = vec!["Title\nPrice: 12".to_string(), "Footer\nSigned".to_string()];
        let r = compare_texts(&old, &new);
        assert_eq!((r.added, r.removed), (2, 1));
        assert!(r.lines.contains(&Line {
            change: Change::Removed,
            page: 1,
            text: "Price: 10".into()
        }));
        assert!(r.lines.contains(&Line {
            change: Change::Added,
            page: 1,
            text: "Price: 12".into()
        }));
        assert!(r.lines.contains(&Line {
            change: Change::Added,
            page: 2,
            text: "Signed".into()
        }));
    }

    #[test]
    fn identical_documents_have_no_changes() {
        let pages = vec!["a\nb".to_string()];
        let r = compare_texts(&pages, &pages);
        assert_eq!((r.added, r.removed), (0, 0));
        assert!(r.lines.iter().all(|l| l.change == Change::Same));
    }

    #[test]
    fn empty_documents() {
        let r = compare_texts(&[], &["x".to_string()]);
        assert_eq!((r.added, r.removed), (1, 0));
    }
}
