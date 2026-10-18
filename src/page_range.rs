//! Parses page selections typed by people ("1, 3-5, 9-") into zero-based indices.

use anyhow::{Result, bail};

/// Parses a 1-based selection such as `"1,3-5,8-"` against a document with `page_count`
/// pages. Returns zero-based indices in the order given, without duplicates.
/// An empty or `"all"` selection means every page.
pub fn parse(spec: &str, page_count: usize) -> Result<Vec<usize>> {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("all") {
        return Ok((0..page_count).collect());
    }
    let mut out: Vec<usize> = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (start, end) = match part.split_once('-') {
            Some((a, b)) => {
                let a = if a.trim().is_empty() {
                    1
                } else {
                    number(a, page_count)?
                };
                let b = if b.trim().is_empty() {
                    page_count
                } else {
                    number(b, page_count)?
                };
                (a, b)
            }
            None => {
                let n = number(part, page_count)?;
                (n, n)
            }
        };
        if start > end {
            bail!("range \"{part}\" runs backwards");
        }
        for page in start..=end {
            if !out.contains(&(page - 1)) {
                out.push(page - 1);
            }
        }
    }
    if out.is_empty() {
        bail!("no pages selected");
    }
    Ok(out)
}

/// Splits a selection into separate groups, one per comma-separated part.
/// `"1-3, 4-6"` becomes two groups. Used by Split.
pub fn parse_groups(spec: &str, page_count: usize) -> Result<Vec<Vec<usize>>> {
    spec.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|part| parse(part, page_count))
        .collect::<Result<Vec<_>>>()
        .and_then(|groups| {
            if groups.is_empty() {
                bail!("no pages selected")
            } else {
                Ok(groups)
            }
        })
}

fn number(text: &str, page_count: usize) -> Result<usize> {
    let n: usize = text
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("\"{}\" is not a page number", text.trim()))?;
    if n == 0 || n > page_count {
        bail!("page {n} does not exist (document has {page_count} pages)");
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_all_select_everything() {
        assert_eq!(parse("", 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(parse(" ALL ", 2).unwrap(), vec![0, 1]);
    }

    #[test]
    fn singles_ranges_and_open_ends() {
        assert_eq!(parse("2, 4-5", 6).unwrap(), vec![1, 3, 4]);
        assert_eq!(parse("5-", 6).unwrap(), vec![4, 5]);
        assert_eq!(parse("-2", 6).unwrap(), vec![0, 1]);
    }

    #[test]
    fn keeps_order_and_drops_duplicates() {
        assert_eq!(parse("3,1,3,1-2", 3).unwrap(), vec![2, 0, 1]);
    }

    #[test]
    fn boundaries() {
        assert!(parse("0", 3).is_err());
        assert_eq!(parse("3", 3).unwrap(), vec![2]);
        assert!(parse("4", 3).is_err());
        assert!(parse("3-1", 3).is_err());
        assert!(parse("x", 3).is_err());
        assert!(parse(",", 3).is_err());
    }

    #[test]
    fn groups() {
        assert_eq!(
            parse_groups("1-2, 3", 3).unwrap(),
            vec![vec![0, 1], vec![2]]
        );
        assert!(parse_groups(" , ", 3).is_err());
    }
}
