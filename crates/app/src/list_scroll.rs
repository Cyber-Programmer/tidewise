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

/// Where the selection lands after moving `delta` rows (negative = up), stopping at the first or last row.
///
/// `current` is the selected row's position, or `None` when nothing is selected yet (that counts as the first
/// row). Returns `None` only when there are no rows at all.
pub fn moved_selection(current: Option<usize>, row_count: usize, delta: isize) -> Option<usize> {
    let last = row_count.checked_sub(1)?;
    Some(current.unwrap_or(0).saturating_add_signed(delta).min(last))
}

/// A key that scrolls a list nothing can be selected in (the review list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// The scroll offset after `key`: a line, a visible page or an end, never past the content.
pub fn scroll_by_key(key: ScrollKey, current: f32, line: f32, content: f32, visible: f32) -> f32 {
    let furthest = (content - visible).max(0.0);
    let wanted = match key {
        ScrollKey::Up => current - line,
        ScrollKey::Down => current + line,
        ScrollKey::PageUp => current - visible,
        ScrollKey::PageDown => current + visible,
        ScrollKey::Home => 0.0,
        ScrollKey::End => furthest,
    };
    wanted.clamp(0.0, furthest)
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
    fn scroll_keys_move_a_line_a_page_or_to_an_end() {
        // 1000 px of content in a 300 px box: the furthest offset is 700.
        let after = |key, current| scroll_by_key(key, current, 53.0, 1000.0, 300.0);
        assert_eq!(after(ScrollKey::Down, 0.0), 53.0);
        assert_eq!(after(ScrollKey::Up, 106.0), 53.0);
        assert_eq!(after(ScrollKey::PageDown, 53.0), 353.0);
        assert_eq!(after(ScrollKey::PageUp, 353.0), 53.0);
        assert_eq!(after(ScrollKey::End, 0.0), 700.0);
        assert_eq!(after(ScrollKey::Home, 700.0), 0.0);
    }

    #[test]
    fn scroll_keys_stop_at_both_ends_and_content_that_fits_never_moves() {
        let after = |key, current| scroll_by_key(key, current, 53.0, 1000.0, 300.0);
        assert_eq!(after(ScrollKey::Up, 20.0), 0.0);
        assert_eq!(after(ScrollKey::Down, 690.0), 700.0);
        assert_eq!(after(ScrollKey::PageDown, 600.0), 700.0);
        assert_eq!(scroll_by_key(ScrollKey::End, 0.0, 53.0, 100.0, 300.0), 0.0);
        assert_eq!(scroll_by_key(ScrollKey::Down, 0.0, 53.0, 100.0, 300.0), 0.0);
    }

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
    fn moving_the_selection_steps_and_stops_at_the_ends() {
        assert_eq!(moved_selection(Some(2), 5, 1), Some(3));
        assert_eq!(moved_selection(Some(2), 5, -1), Some(1));
        assert_eq!(moved_selection(Some(4), 5, 1), Some(4));
        assert_eq!(moved_selection(Some(0), 5, -1), Some(0));
        assert_eq!(moved_selection(Some(2), 5, 100), Some(4));
        assert_eq!(moved_selection(Some(2), 5, -100), Some(0));
        assert_eq!(moved_selection(Some(2), 5, isize::MAX), Some(4));
        assert_eq!(moved_selection(Some(2), 5, isize::MIN), Some(0));
        assert_eq!(moved_selection(Some(2), 5, 0), Some(2));
    }

    #[test]
    fn with_nothing_selected_the_first_row_is_the_starting_point() {
        assert_eq!(moved_selection(None, 5, 0), Some(0));
        assert_eq!(moved_selection(None, 5, 2), Some(2));
    }

    #[test]
    fn moving_the_selection_with_no_rows_gives_nothing() {
        assert_eq!(moved_selection(None, 0, 1), None);
        assert_eq!(moved_selection(Some(0), 0, -1), None);
    }

    #[test]
    fn the_arrow_keys_between_checkbox_rows_move_one_row_and_stop_at_the_ends() {
        // Up/Down on a checkbox row move the focus with the same function as the package list's selection.
        // Documenting cases for the group card: they pass already, `moved_selection` is the one function.
        // first row, Up: stays
        assert_eq!(moved_selection(Some(0), 62, -1), Some(0));
        // last row, Down: stays (the real `plasma` has 62 members)
        assert_eq!(moved_selection(Some(61), 62, 1), Some(61));
        // in the middle
        assert_eq!(moved_selection(Some(30), 62, 1), Some(31));
        assert_eq!(moved_selection(Some(30), 62, -1), Some(29));
        // a single row: both keys stay on it
        assert_eq!(moved_selection(Some(0), 1, 1), Some(0));
        assert_eq!(moved_selection(Some(0), 1, -1), Some(0));
        // no rows: nothing to focus
        assert_eq!(moved_selection(Some(0), 0, 1), None);
    }

    #[test]
    fn a_long_choice_list_in_a_bounded_box_follows_the_focused_row() {
        // The install screens' lists: 44px rows plus a 1px separator = 45px, in a box that shows 3 rows (135px).
        // 62 group members (the real `plasma`) must all be reachable by Tab.
        const PITCH: f32 = 45.0;
        assert_eq!(scroll_to_show(2, PITCH, 135.0, 0.0), 0.0);
        assert_eq!(scroll_to_show(3, PITCH, 135.0, 0.0), 45.0);
        assert_eq!(scroll_to_show(61, PITCH, 135.0, 0.0), 62.0 * PITCH - 135.0);
        // Shift+Tab back to the top row
        assert_eq!(scroll_to_show(0, PITCH, 135.0, 2_655.0), 0.0);
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
