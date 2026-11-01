//! End task: what the dialogs say. Plain data; `endtask_live.rs` shows it and runs the signals.

use tidewise_adapters::cleanup::format_bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    pub title: String,
    pub text: String,
    /// The red button, or empty when there is none.
    pub confirm: String,
}

/// `End Firefox?` and what may be lost.
pub fn confirm(name: &str, count: usize) -> Texts {
    let _ = (name, count);
    Texts {
        title: String::new(),
        text: String::new(),
        confirm: String::new(),
    }
}

/// The app did not close after the polite request.
pub fn frozen(name: &str, waited_secs: u64, remaining: usize) -> Texts {
    let _ = (name, waited_secs, remaining);
    Texts {
        title: String::new(),
        text: String::new(),
        confirm: String::new(),
    }
}

/// A protected part of the system: `id` is the group key (`plasma`, `sound`, …).
pub fn protected(id: &str, name: &str) -> Texts {
    let _ = (id, name);
    Texts {
        title: String::new(),
        text: String::new(),
        confirm: String::new(),
    }
}

/// The toast after the app ended.
pub fn ended(name: &str, freed: u64) -> String {
    let _ = (name, freed, format_bytes(0));
    String::new()
}

/// Technical details of the "not closing" dialog.
pub fn details(sent: usize, exited: usize, still_running: &[u32]) -> String {
    let _ = (sent, exited, still_running);
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_says_how_many_processes_close_and_that_work_may_be_lost() {
        let t = confirm("Firefox", 14);
        assert_eq!(t.title, "End Firefox?");
        assert_eq!(t.text, "All 14 Firefox processes will close. Anything you haven't saved, like a half-written form, may be lost.");
        assert_eq!(t.confirm, "End Firefox");
        assert_eq!(confirm("Kdenlive", 1).text, "Kdenlive will close. Anything you haven't saved, like a half-written form, may be lost.");
        assert_eq!(confirm("Kdenlive", 2).text, "Both Kdenlive processes will close. Anything you haven't saved, like a half-written form, may be lost.");
    }

    #[test]
    fn frozen_says_how_long_we_waited_and_what_force_quit_does() {
        let t = frozen("Firefox", 5, 2);
        assert_eq!(t.title, "Firefox isn't closing");
        assert_eq!(t.text, "We asked it to close 5 seconds ago and 2 of its processes are still running. It may be stuck. Force quitting stops it immediately, without letting it save.");
        assert_eq!(t.confirm, "Force quit");
        assert!(frozen("Firefox", 5, 1)
            .text
            .contains("and 1 of its processes is still running"));
        assert!(frozen("Firefox", 1, 1).text.contains("1 second ago"));
        assert!(
            frozen("Solo", 5, 1)
                .text
                .contains("and it is still running")
                || frozen("Solo", 5, 1).text.contains("1 of its processes is")
        );
    }

    #[test]
    fn protected_parts_say_why_and_offer_no_end_button() {
        let t = protected("plasma", "Plasma desktop");
        assert_eq!(t.title, "Plasma desktop is protected");
        assert!(t
            .text
            .starts_with("It draws your desktop, panel and windows."));
        assert!(t.text.contains("logging out and back in is the safe fix"));
        assert_eq!(t.confirm, "");
        let other = protected("sound", "Sound system");
        assert_eq!(other.title, "Sound system is protected");
        assert!(other.text.starts_with("Your system needs it to run."));
    }

    #[test]
    fn the_toast_names_the_app_and_the_memory_it_gave_back() {
        assert_eq!(
            ended("Firefox", 2_100_000_000),
            "Firefox ended · 2.1 GB of memory freed"
        );
        assert_eq!(
            ended("Firefox", 430_000_000),
            "Firefox ended · 430 MB of memory freed"
        );
        assert_eq!(ended("Firefox", 0), "Firefox ended");
    }

    #[test]
    fn details_name_the_processes_that_are_still_running() {
        assert_eq!(
            details(14, 12, &[2231, 2298]),
            "sent SIGTERM to 14 processes · 12 exited\nstill running: PID 2231, 2298 · next: SIGKILL"
        );
        assert_eq!(
            details(1, 0, &[7]),
            "sent SIGTERM to 1 process · 0 exited\nstill running: PID 7 · next: SIGKILL"
        );
    }
}
