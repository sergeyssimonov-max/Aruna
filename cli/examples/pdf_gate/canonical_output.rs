//! An unsuccessful or silent canonicalizer cannot establish XML equality.

use std::process::Output;

pub(super) fn checked_bytes(output: &Output) -> Result<&[u8], String> {
    if !output.status.success() {
        return Err(format!(
            "xmllint failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.is_empty() {
        return Err("xmllint returned no canonical XML".into());
    }
    Ok(&output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn output(code: i32, bytes: &[u8]) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: bytes.to_vec(),
            stderr: b"XML parser diagnostic".to_vec(),
        }
    }

    #[test]
    fn successful_canonical_xml_is_compared_byte_for_byte() {
        let result = output(0, b"<root></root>");
        assert_eq!(checked_bytes(&result).unwrap(), b"<root></root>");
    }

    #[test]
    fn parser_failure_is_an_error_even_when_it_wrote_partial_output() {
        for bytes in [b"".as_slice(), b"<root>".as_slice()] {
            let result = output(1, bytes);
            let error = checked_bytes(&result).unwrap_err();
            assert!(error.contains("xmllint failed"));
            assert!(error.contains("XML parser diagnostic"));
        }
    }

    #[test]
    fn missing_canonical_output_cannot_equal_another_missing_output() {
        let result = output(0, b"");
        assert!(checked_bytes(&result)
            .unwrap_err()
            .contains("no canonical XML"));
    }
}
