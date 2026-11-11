//! View model for the Apps screen: tabs, search, sort, selection and details.
//! Plain Rust. It never runs pacman; it is handed the result of `tidewise_pacman::installed_packages()`.

/// Sizes the way people read them: SI units (1 kB = 1 000 bytes), so "1000 kB" never appears.
///
/// Integer maths only. Each unit is rounded first; if that gives 1000 of the unit, the next unit is used.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1_000 {
        return format!("{bytes} B");
    }
    let kilobytes = divide_rounded(bytes, 1_000);
    if kilobytes < 1_000 {
        return format!("{kilobytes} kB");
    }
    let tenths_of_mb = divide_rounded(bytes, 100_000);
    if tenths_of_mb < 10_000 {
        return format!("{}.{} MB", tenths_of_mb / 10, tenths_of_mb % 10);
    }
    let hundredths_of_gb = divide_rounded(bytes, 10_000_000);
    format!(
        "{}.{:02} GB",
        hundredths_of_gb / 100,
        hundredths_of_gb % 100
    )
}

/// `number / unit`, rounded to the nearest whole number (halves go up). `unit` is never 0 here.
fn divide_rounded(number: u64, unit: u64) -> u64 {
    let whole = number / unit;
    if number % unit >= unit / 2 {
        whole + 1
    } else {
        whole
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_sizes_are_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
    }

    #[test]
    fn kilobytes_are_whole_numbers_rounded_to_nearest() {
        assert_eq!(format_size(1000), "1 kB");
        assert_eq!(format_size(1499), "1 kB");
        assert_eq!(format_size(1500), "2 kB");
        assert_eq!(format_size(999_499), "999 kB");
    }

    #[test]
    fn rounding_up_moves_to_the_next_unit() {
        // 999 500 bytes would round to "1000 kB": it must become "1.0 MB".
        assert_eq!(format_size(999_500), "1.0 MB");
        assert_eq!(format_size(999_949_999), "999.9 MB");
        // 999 950 000 bytes would round to "1000.0 MB": it must become "1.00 GB".
        assert_eq!(format_size(999_950_000), "1.00 GB");
    }

    #[test]
    fn megabytes_have_one_decimal_and_gigabytes_two() {
        assert_eq!(format_size(1_250_000), "1.3 MB");
        assert_eq!(format_size(12_345_678_901), "12.35 GB");
    }

    #[test]
    fn the_largest_number_does_not_panic() {
        assert_eq!(format_size(u64::MAX), "18446744073.71 GB");
    }
}
