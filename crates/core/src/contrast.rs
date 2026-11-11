//! WCAG 2.x contrast ratio between two colours. Keeps the design tokens readable.

/// Parses "#RRGGBB" into [r, g, b]. Returns None for anything else. Must never panic.
pub fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let s = hex.strip_prefix('#')?;
    if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Contrast ratio from 1.0 (same colour) to 21.0 (black on white). Order doesn't matter.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// How bright a colour looks to the eye, from 0.0 (black) to 1.0 (white).
fn luminance([r, g, b]: [u8; 3]) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Undoes the sRGB "gamma" of one channel so brightness can be added up.
fn linear(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: [u8; 3] = [0, 0, 0];
    const WHITE: [u8; 3] = [255, 255, 255];

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected:.2}, got {actual:.4}"
        );
    }

    #[test]
    fn parses_hex_colours() {
        assert_eq!(parse_hex("#0E6A6D"), Some([0x0E, 0x6A, 0x6D]));
        assert_eq!(parse_hex("#ffffff"), Some(WHITE));
    }

    #[test]
    fn rejects_bad_hex() {
        assert_eq!(parse_hex("0E6A6D"), None); // no #
        assert_eq!(parse_hex("#0E6A6"), None); // too short
        assert_eq!(parse_hex("#GG0000"), None); // not hex digits
        assert_eq!(parse_hex("#0é000"), None); // multi-byte character: must not panic
        assert_eq!(parse_hex("#+F0000"), None); // a sign is not a hex digit
    }

    #[test]
    fn black_on_white_is_21() {
        assert_close(contrast_ratio(BLACK, WHITE), 21.0);
    }

    #[test]
    fn order_does_not_matter() {
        assert_close(contrast_ratio(WHITE, BLACK), 21.0);
    }

    #[test]
    fn same_colour_is_1() {
        assert_close(contrast_ratio(WHITE, WHITE), 1.0);
    }

    #[test]
    fn known_grey_on_white() {
        // #767676 on white is the classic "just passes AA" grey: 4.54:1.
        assert_close(contrast_ratio([0x76; 3], WHITE), 4.54);
    }
}
