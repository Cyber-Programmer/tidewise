//! The Settings screen's small pieces of logic: which theme button is on and what the update-check
//! line says. Plain data; `settings_live.rs` shows it and saves the changes.

use tidewise_core::settings::ThemeChoice;

/// 0 follow the system, 1 dark, 2 light: the index of the button, and what `Theme.mode` means in Slint.
pub fn theme_index(theme: ThemeChoice) -> i32 {
    match theme {
        ThemeChoice::System => 0,
        ThemeChoice::Dark => 1,
        ThemeChoice::Light => 2,
    }
}

/// The button that was pressed; anything unknown means "follow the system".
pub fn theme_from_index(i: i32) -> ThemeChoice {
    match i {
        1 => ThemeChoice::Dark,
        2 => ThemeChoice::Light,
        _ => ThemeChoice::System,
    }
}

/// The words under the update-check switch. Tidewise has no update check yet, and nothing is ever
/// sent anywhere: the sentence must say so.
pub fn update_note(on: bool) -> String {
    if on {
        "Tidewise has no update check yet. When it has one, it will look for a new version once a day and do nothing else. Nothing is sent anywhere.".into()
    } else {
        "Tidewise has no update check yet. With this off it will never look for updates by itself. Nothing is sent anywhere.".into()
    }
}

/// `Tidewise 0.1.0`
pub fn about_title(version: &str) -> String {
    format!("Tidewise {version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_map_to_the_themes_and_back() {
        for (i, t) in [
            (0, ThemeChoice::System),
            (1, ThemeChoice::Dark),
            (2, ThemeChoice::Light),
        ] {
            assert_eq!(theme_index(t), i);
            assert_eq!(theme_from_index(i), t);
        }
        for odd in [-1, 3, 99] {
            assert_eq!(theme_from_index(odd), ThemeChoice::System);
        }
    }

    #[test]
    fn the_update_line_is_honest_with_the_switch_on_or_off() {
        let on = update_note(true);
        let off = update_note(false);
        for text in [&on, &off] {
            assert!(text.contains("no update check yet"), "{text}");
            assert!(text.contains("Nothing is sent anywhere"), "{text}");
        }
        assert!(on.contains("once a day"), "{on}");
        assert!(off.contains("never"), "{off}");
    }

    #[test]
    fn the_about_title_has_the_name_and_version() {
        assert_eq!(about_title("0.1.0"), "Tidewise 0.1.0");
    }
}
