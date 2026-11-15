//! Validate the independent reader's response before interpreting its labels.
//!
//! A missing row or a failed reader is an unmet check, not evidence that a
//! deliberately corrupted PDF was caught.

use std::path::Path;
use std::process::Output;

pub(super) fn checked_rows<'a>(
    output: &'a Output,
    paths: &[&Path],
) -> Result<Vec<&'a str>, String> {
    if !output.status.success() {
        return Err(format!(
            "PDFKit reader failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|e| format!("PDFKit reader returned invalid UTF-8: {e}"))?;
    let rows: Vec<&str> = text.lines().collect();
    if rows.len() != paths.len() {
        return Err(format!(
            "PDFKit reader returned {} rows for {} PDFs",
            rows.len(),
            paths.len()
        ));
    }
    for (path, row) in paths.iter().zip(&rows) {
        let fields: Vec<&str> = row.split('\t').collect();
        if fields.get(1).copied() != path.to_str() {
            return Err(format!(
                "PDFKit reader row does not name {}: {row}",
                path.display()
            ));
        }
        let valid = match fields.as_slice() {
            ["UNREADABLE", _] => true,
            ["OPENED", _, pages, labels, letters] => {
                pages.parse::<usize>().is_ok_and(|n| n > 0)
                    && letters.parse::<usize>().is_ok()
                    && (labels.is_empty()
                        || labels.split(',').all(|label| {
                            label.split_once(':').is_some_and(|(cp, count)| {
                                u32::from_str_radix(cp, 16).is_ok()
                                    && count.parse::<usize>().is_ok()
                            })
                        }))
            }
            _ => false,
        };
        if !valid {
            return Err(format!("PDFKit reader returned a malformed row: {row}"));
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn output(code: i32, text: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: text.as_bytes().to_vec(),
            stderr: b"reader diagnostic".to_vec(),
        }
    }

    #[test]
    fn complete_rows_belong_to_the_requested_pdfs() {
        let text = "OPENED\t/tmp/one.pdf\t2\t100009:1\t0\nUNREADABLE\t/tmp/two.pdf\n";
        let result = output(0, text);
        let paths = [Path::new("/tmp/one.pdf"), Path::new("/tmp/two.pdf")];
        assert_eq!(
            checked_rows(&result, &paths).unwrap(),
            text.lines().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_failed_reader_is_not_a_caught_negative_control() {
        let result = output(1, "UNREADABLE\t/tmp/one.pdf\n");
        assert!(checked_rows(&result, &[Path::new("/tmp/one.pdf")])
            .unwrap_err()
            .contains("reader failed"));
    }

    #[test]
    fn invalid_utf8_is_not_silently_replaced_in_reader_output() {
        let mut result = output(0, "OPENED\t/tmp/one.pdf\t1\t\t0\n");
        result.stdout[0] = 0xff;
        assert!(checked_rows(&result, &[Path::new("/tmp/one.pdf")])
            .unwrap_err()
            .contains("invalid UTF-8"));
    }

    #[test]
    fn missing_and_extra_rows_fail_instead_of_truncating_the_comparison() {
        let paths = [Path::new("/tmp/one.pdf"), Path::new("/tmp/two.pdf")];
        for text in [
            "",
            "OPENED\t/tmp/one.pdf\t1\t\t0\n",
            "OPENED\t/tmp/one.pdf\t1\t\t0\nOPENED\t/tmp/two.pdf\t1\t\t0\nextra\n",
        ] {
            assert!(checked_rows(&output(0, text), &paths)
                .unwrap_err()
                .contains("rows for"));
        }
    }

    #[test]
    fn a_row_for_another_path_cannot_stand_in_for_the_requested_pdf() {
        let result = output(0, "OPENED\t/tmp/other.pdf\t1\t\t0\n");
        assert!(checked_rows(&result, &[Path::new("/tmp/one.pdf")])
            .unwrap_err()
            .contains("does not name"));
    }

    #[test]
    fn malformed_rows_are_reader_failures_not_pdf_failures() {
        for text in [
            "OPENED\t/tmp/one.pdf\t0\t\t0\n",
            "OPENED\t/tmp/one.pdf\t1\tbad-label\t0\n",
            "OPENED\t/tmp/one.pdf\t1\t\tunknown\n",
            "diagnostic\t/tmp/one.pdf\n",
        ] {
            assert!(checked_rows(&output(0, text), &[Path::new("/tmp/one.pdf")])
                .unwrap_err()
                .contains("malformed row"));
        }
    }
}
