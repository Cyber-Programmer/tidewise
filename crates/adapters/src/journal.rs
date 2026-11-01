//! Size of the systemd journal, from `journalctl --disk-usage` (readable without root).

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("no size like `2.8G` found in the journalctl output")]
    NoSize,
}

/// Bytes the journals take, from a line like
/// `Archived and active journals take up 2.8G in the file system.`
pub fn parse_disk_usage(text: &str) -> Result<u64, ParseError> {
    text.split_whitespace()
        .find_map(parse_size)
        .ok_or(ParseError::NoSize)
}

/// `2.8G`, `1,5G`, `0B`: digits, optional decimal part, one unit letter. journalctl counts in
/// powers of 1024 and, in other languages, may print a comma.
fn parse_size(token: &str) -> Option<u64> {
    let token = token.trim_end_matches(['.', ',']);
    let unit = token.chars().last()?;
    let power: i32 = match unit {
        'B' => 0,
        'K' => 1,
        'M' => 2,
        'G' => 3,
        'T' => 4,
        'P' => 5,
        _ => return None,
    };
    let number = &token[..token.len() - 1];
    let valid = !number.is_empty()
        && number
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        && number.chars().next().is_some_and(|c| c.is_ascii_digit())
        && number.chars().filter(|c| matches!(c, '.' | ',')).count() <= 1;
    if !valid {
        return None;
    }
    let value: f64 = number.replace(',', ".").parse().ok()?;
    let bytes = value * 1024f64.powi(power);
    // Anything past an exabyte is not a journal size.
    (bytes.is_finite() && bytes < 1e18).then(|| bytes.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/junk/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn real_output_is_in_binary_units() {
        // 2.8G = 2.8 * 1024^3 (journalctl uses powers of 1024).
        assert_eq!(
            parse_disk_usage(&fixture("journal-real.txt")),
            Ok(3_006_477_107)
        );
    }

    #[test]
    fn german_wording_and_comma_decimal() {
        assert_eq!(
            parse_disk_usage(&fixture("journal-de.txt")),
            Ok(1_610_612_736)
        );
    }

    #[test]
    fn zero_bytes() {
        assert_eq!(parse_disk_usage(&fixture("journal-zero.txt")), Ok(0));
    }

    #[test]
    fn broken_or_empty_output_is_an_error() {
        assert_eq!(
            parse_disk_usage(&fixture("journal-broken.txt")),
            Err(ParseError::NoSize)
        );
        assert_eq!(parse_disk_usage(""), Err(ParseError::NoSize));
        assert_eq!(parse_disk_usage("take up 5X in"), Err(ParseError::NoSize));
        assert_eq!(
            parse_disk_usage("take up 99999999999999999999T"),
            Err(ParseError::NoSize)
        );
    }
}
