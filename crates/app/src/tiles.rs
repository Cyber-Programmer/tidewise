//! How a block of the folder map lays out its text, from its size in pixels. Plain Rust.

/// How the text sits in the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Name at the top, size at the bottom.
    Stacked,
    /// As `Stacked`, plus the "not opened in 6 months" note under the size.
    StackedNote,
    /// Name and size on one line; the name shortens first, the size never.
    OneLine,
    /// Too small for text: only the accessible label and a tooltip.
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub mode: Mode,
    pub name_px: f32,
    pub size_px: f32,
}

/// Padding around the text.
pub fn pad(_tier: i32) -> f32 {
    0.0
}

/// Font size of the name.
pub fn name_px(_tier: i32) -> f32 {
    0.0
}

/// Font sizes the size text may take for a tier, biggest first.
pub fn size_steps(_tier: i32) -> Vec<f32> {
    Vec::new()
}

/// Height a stacked block needs.
pub fn stacked_height(_tier: i32, _size_px: f32, _note: bool) -> f32 {
    0.0
}

/// Smallest block that still holds name and size on one line: (width for `size_chars`, height).
pub fn one_line_min(_tier: i32, _size_chars: usize) -> (f32, f32) {
    (0.0, 0.0)
}

pub fn tile_layout(_w: f32, _h: f32, tier: i32, _size_chars: usize, _has_note: bool) -> Layout {
    Layout {
        mode: Mode::Stacked,
        name_px: name_px(tier),
        size_px: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f32 = 400.0;

    #[test]
    fn a_tall_block_stacks_name_over_size_at_its_tier_size() {
        let l = tile_layout(WIDE, 300.0, 0, 7, false);
        assert_eq!(l.mode, Mode::Stacked);
        assert_eq!((l.name_px, l.size_px), (14.0, 24.0));
        let l = tile_layout(WIDE, 300.0, 3, 6, false);
        assert_eq!((l.name_px, l.size_px), (13.0, 16.0));
    }

    #[test]
    fn the_note_shows_only_when_it_fits_under_the_size() {
        let need = stacked_height(1, 22.0, true);
        assert_eq!(tile_layout(WIDE, need, 1, 7, true).mode, Mode::StackedNote);
        let plain = tile_layout(WIDE, need - 1.0, 1, 7, true);
        assert_eq!(
            plain.mode,
            Mode::Stacked,
            "one pixel short: no note, still stacked"
        );
        assert_eq!(tile_layout(WIDE, 300.0, 1, 7, false).mode, Mode::Stacked);
    }

    #[test]
    fn a_stacked_block_is_exactly_as_tall_as_its_text() {
        for tier in 0..=3 {
            let size = size_steps(tier)[0];
            let need = stacked_height(tier, size, false);
            assert!(
                need > 2.0 * pad(tier) + name_px(tier) + size,
                "tier {tier}: {need}"
            );
            assert_eq!(
                tile_layout(WIDE, need, tier, 6, false).mode,
                Mode::Stacked,
                "tier {tier} at its height"
            );
            assert_ne!(
                tile_layout(WIDE, need - 0.5, tier, 6, false).size_px,
                size,
                "tier {tier}: half a pixel less must not keep the same size"
            );
        }
    }

    #[test]
    fn the_size_steps_down_until_the_block_can_hold_it() {
        let big = size_steps(0);
        assert_eq!(big[0], 24.0);
        assert!(big.windows(2).all(|w| w[0] > w[1]), "steps shrink: {big:?}");
        let h = stacked_height(0, big[1], false);
        let l = tile_layout(WIDE, h, 0, 7, false);
        assert_eq!((l.mode, l.size_px), (Mode::Stacked, big[1]));
    }

    #[test]
    fn a_short_block_puts_name_and_size_on_one_line() {
        // The "Android" block of the live screenshot: 65 px tall.
        let l = tile_layout(300.0, 65.0, 3, 7, false);
        assert_eq!(l.mode, Mode::OneLine);
        assert!(l.size_px <= 16.0 && l.name_px <= 14.0);
    }

    #[test]
    fn one_line_has_a_smallest_height_and_width() {
        let (w, h) = one_line_min(3, 7);
        assert!(w > 0.0 && h > 0.0);
        assert_eq!(
            tile_layout(w, h, 3, 7, false).mode,
            Mode::OneLine,
            "exactly the minimum"
        );
        assert_eq!(
            tile_layout(w, h - 0.5, 3, 7, false).mode,
            Mode::Hidden,
            "a bit too short"
        );
        assert_eq!(
            tile_layout(w - 0.5, h, 3, 7, false).mode,
            Mode::Hidden,
            "a bit too narrow"
        );
        let (wide, _) = one_line_min(3, 9);
        assert!(wide > w, "a longer size needs more room");
    }

    #[test]
    fn a_tiny_block_has_no_text() {
        assert_eq!(tile_layout(20.0, 20.0, 3, 6, false).mode, Mode::Hidden);
        assert_eq!(tile_layout(0.0, 0.0, 3, 6, true).mode, Mode::Hidden);
        assert_eq!(tile_layout(-5.0, 100.0, 0, 6, false).mode, Mode::Hidden);
        assert_eq!(tile_layout(f32::NAN, 100.0, 0, 6, false).mode, Mode::Hidden);
    }

    #[test]
    fn no_block_size_makes_text_overlap_or_leave_the_block() {
        for tier in 0..=3 {
            for chars in [4usize, 6, 8, 9] {
                for note in [false, true] {
                    let (mut w, mut h);
                    h = 8.0;
                    while h <= 400.0 {
                        w = 8.0;
                        while w <= 500.0 {
                            let l = tile_layout(w, h, tier, chars, note);
                            let p = pad(tier);
                            match l.mode {
                                Mode::Stacked | Mode::StackedNote => {
                                    let n = l.mode == Mode::StackedNote;
                                    assert!(
                                        h >= stacked_height(tier, l.size_px, n),
                                        "stacked {w}x{h} tier {tier}"
                                    );
                                    assert!(
                                        w >= 2.0 * p + chars as f32 * 0.55 * l.size_px,
                                        "size too wide at {w}x{h}"
                                    );
                                }
                                Mode::OneLine => {
                                    let (mw, mh) = one_line_min(tier, chars);
                                    assert!(w >= mw && h >= mh, "one line at {w}x{h}");
                                }
                                Mode::Hidden => {}
                            }
                            w += 7.0;
                        }
                        h += 7.0;
                    }
                }
            }
        }
    }
}
