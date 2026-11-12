//! Scrolling maths for the package list: which scroll position keeps the selected row on screen.
//!
//! Plain numbers, no Slint types. Offsets here are positive ("scrolled down by this many pixels");
//! Slint's `content-y` is negative when scrolled down, so the two small functions at the bottom
//! convert between them. That is the only place the sign flips.

/// The scroll offset that shows the whole row `index`, moving as little as possible.
/// If the row is already fully visible, `current_offset` comes back unchanged.
pub fn scroll_to_show(
    index: usize,
    row_height: f32,
    visible_height: f32,
    current_offset: f32,
) -> f32 {
    if row_height <= 0.0 || visible_height <= 0.0 {
        return current_offset;
    }
    let top = index as f32 * row_height;
    let bottom = top + row_height;
    if top < current_offset || row_height > visible_height {
        top
    } else if bottom > current_offset + visible_height {
        bottom - visible_height
    } else {
        current_offset
    }
}

/// How many whole rows fit in the visible area (at least 1, so Page Up/Down always move).
pub fn rows_per_page(visible_height: f32, row_height: f32) -> usize {
    if row_height <= 0.0 || visible_height <= 0.0 {
        return 1;
    }
    ((visible_height / row_height) as usize).max(1)
}

/// Slint's `content-y` for a positive scroll offset.
pub fn offset_to_content_y(offset: f32) -> f32 {
    if offset <= 0.0 { 0.0 } else { -offset }
}

/// The positive scroll offset for Slint's `content-y` (never negative).
pub fn content_y_to_offset(content_y: f32) -> f32 {
    if content_y >= 0.0 { 0.0 } else { -content_y }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: f32 = 64.0;

    #[test]
    fn a_row_that_is_fully_visible_does_not_scroll() {
        // 640 px visible = rows 0..=9 at offset 0; rows 2..=11 at offset 128.
        assert_eq!(scroll_to_show(0, ROW, 640.0, 0.0), 0.0);
        assert_eq!(scroll_to_show(9, ROW, 640.0, 0.0), 0.0);
        assert_eq!(scroll_to_show(2, ROW, 640.0, 128.0), 128.0);
        assert_eq!(scroll_to_show(11, ROW, 640.0, 128.0), 128.0);
    }

    #[test]
    fn a_row_below_the_view_scrolls_down_just_enough() {
        // Row 10 spans 640..704; its bottom must reach the bottom edge: offset 704 - 640 = 64.
        assert_eq!(scroll_to_show(10, ROW, 640.0, 0.0), 64.0);
        assert_eq!(scroll_to_show(1499, ROW, 640.0, 0.0), 1500.0 * 64.0 - 640.0);
    }

    #[test]
    fn a_row_above_the_view_scrolls_up_just_enough() {
        // Row 3 starts at 192; the view starts at 400.
        assert_eq!(scroll_to_show(3, ROW, 640.0, 400.0), 192.0);
        assert_eq!(scroll_to_show(0, ROW, 640.0, 400.0), 0.0);
    }

    #[test]
    fn a_partly_visible_row_is_brought_fully_into_view() {
        // View 32..672: row 0 is cut at the top, row 10 (640..704) is cut at the bottom.
        assert_eq!(scroll_to_show(0, ROW, 640.0, 32.0), 0.0);
        assert_eq!(scroll_to_show(10, ROW, 640.0, 32.0), 64.0);
    }

    #[test]
    fn an_unmeasured_or_tiny_view_leaves_the_scroll_alone() {
        assert_eq!(scroll_to_show(5, ROW, 0.0, 123.0), 123.0);
        assert_eq!(scroll_to_show(5, ROW, -1.0, 123.0), 123.0);
        assert_eq!(scroll_to_show(5, 0.0, 640.0, 123.0), 123.0);
    }

    #[test]
    fn a_row_taller_than_the_view_shows_its_top() {
        assert_eq!(scroll_to_show(3, 100.0, 50.0, 0.0), 300.0);
        assert_eq!(scroll_to_show(3, 100.0, 50.0, 1000.0), 300.0);
    }

    #[test]
    fn a_page_is_the_number_of_whole_rows_that_fit() {
        assert_eq!(rows_per_page(640.0, ROW), 10);
        assert_eq!(rows_per_page(700.0, ROW), 10);
        assert_eq!(rows_per_page(703.9, ROW), 10);
        assert_eq!(rows_per_page(704.0, ROW), 11);
    }

    #[test]
    fn a_page_is_never_less_than_one_row() {
        assert_eq!(rows_per_page(10.0, ROW), 1);
        assert_eq!(rows_per_page(0.0, ROW), 1);
        assert_eq!(rows_per_page(-5.0, ROW), 1);
        assert_eq!(rows_per_page(640.0, 0.0), 1);
    }

    #[test]
    fn content_y_is_the_negative_of_the_offset() {
        assert_eq!(offset_to_content_y(300.0), -300.0);
        assert_eq!(offset_to_content_y(0.0), 0.0);
        assert_eq!(offset_to_content_y(-5.0), 0.0); // an offset is never negative
        assert_eq!(content_y_to_offset(-300.0), 300.0);
        assert_eq!(content_y_to_offset(0.0), 0.0);
        assert_eq!(content_y_to_offset(12.0), 0.0); // pulled past the top: no offset
    }

    #[test]
    fn converting_there_and_back_gives_the_same_offset() {
        for offset in [0.0, 1.0, 64.0, 9_000.5] {
            assert_eq!(content_y_to_offset(offset_to_content_y(offset)), offset);
        }
    }
}
