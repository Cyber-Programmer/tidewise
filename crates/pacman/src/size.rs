//! Turns pacman's "Installed Size" text (like `6.94 MiB`) into bytes.

/// Parses `<digits>.<two digits> <unit>` where unit is `B`, `KiB`, `MiB` or `GiB` (1024-based).
///
/// pacman prints sizes with two decimals, so the result is the nearest whole byte to what pacman
/// showed, not the exact size on disk. Integer maths only; anything odd or too big gives `None`.
pub(crate) fn parse_size(text: &str) -> Option<u64> {
    let (number, unit) = text.split_once(' ')?;
    let unit_bytes: u64 = match unit {
        "B" => 1,
        "KiB" => 1024,
        "MiB" => 1024 * 1024,
        "GiB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    let (whole, hundredths) = number.split_once('.')?;
    // Plain digits only: `parse` alone would also accept a leading "+".
    let all_digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !all_digits(whole) || hundredths.len() != 2 || !all_digits(hundredths) {
        return None;
    }
    // Work in hundredths of a unit so no floating point is needed.
    let in_hundredths = whole
        .parse::<u64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(hundredths.parse::<u64>().ok()?)?;
    // "+ 50" before dividing by 100 rounds to the nearest whole byte.
    let scaled = in_hundredths.checked_mul(unit_bytes)?.checked_add(50)?;
    Some(scaled / 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_zero_bytes() {
        assert_eq!(parse_size("0.00 B"), Some(0));
    }

    #[test]
    fn size_kib_rounds_to_nearest_byte() {
        // 1701.73 * 1024 = 1_742_571.52
        assert_eq!(parse_size("1701.73 KiB"), Some(1_742_572));
    }

    #[test]
    fn size_mib() {
        // 6.94 * 1024 * 1024 = 7_277_117.44
        assert_eq!(parse_size("6.94 MiB"), Some(7_277_117));
    }

    #[test]
    fn size_gib() {
        assert_eq!(parse_size("1.50 GiB"), Some(1_610_612_736));
    }

    #[test]
    fn size_rejects_garbage() {
        for text in [
            "",
            "lots",
            "1.5 TB",
            "-1.00 KiB",
            "+1.00 KiB",
            "1.5 KiB",
            "1.00",
            "1.00 TiB",
            ".50 KiB",
            "1.00  KiB",
            "1.00 kib",
            " 1.00 KiB",
            "1.00 KiB ",
            "é.00 B",
        ] {
            assert_eq!(parse_size(text), None, "should reject {text:?}");
        }
    }

    #[test]
    fn size_overflow_is_none() {
        assert_eq!(parse_size("99999999999999999999.00 GiB"), None);
        assert_eq!(parse_size("18446744073709551615.00 GiB"), None);
    }
}
