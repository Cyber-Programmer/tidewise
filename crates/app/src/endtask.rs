//! End task: what the dialogs say. Plain data; `endtask_live.rs` shows it and runs the signals.

use tidewise_adapters::cleanup::format_bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    pub title: String,
    pub text: String,
    /// The red button, or empty when there is none.
    pub confirm: String,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `End Firefox?` and what may be lost.
pub fn confirm(name: &str, count: usize) -> Texts {
    let who = match count {
        0 | 1 => format!("{name} will close."),
        2 => format!("Both {name} processes will close."),
        n => format!("All {n} {name} processes will close."),
    };
    Texts {
        title: format!("End {name}?"),
        text: format!("{who} Anything you haven't saved, like a half-written form, may be lost."),
        confirm: format!("End {name}"),
    }
}

/// `Force quit Firefox?`: the Force quit button of the Processes details card goes straight to
/// this, without asking the app to close first.
pub fn force_confirm(name: &str, count: usize) -> Texts {
    let who = match count {
        0 | 1 => format!("{name} will stop immediately."),
        2 => format!("Both {name} processes will stop immediately."),
        n => format!("All {n} {name} processes will stop immediately."),
    };
    Texts {
        title: format!("Force quit {name}?"),
        text: format!(
            "{who} Anything you haven't saved will be lost, and it gets no chance to clean up."
        ),
        confirm: "Force quit".into(),
    }
}

/// The dialog while the signal is sent and Tidewise waits (`kill`: SIGKILL, else the polite request).
pub fn closing(name: &str, kill: bool) -> Texts {
    let (title, text) = if kill {
        (format!("Force quitting {name}…"), "Stopping it right away.")
    } else {
        (
            format!("Closing {name}…"),
            "Waiting up to 5 seconds for it to close on its own.",
        )
    };
    Texts {
        title,
        text: text.into(),
        confirm: String::new(),
    }
}

/// The app did not close after the polite request.
pub fn frozen(name: &str, waited_secs: u64, remaining: usize) -> Texts {
    Texts {
        title: format!("{name} isn't closing"),
        text: format!(
            "We asked it to close {} ago and {} of its processes {} still running. It may be stuck. Force quitting stops it immediately, without letting it save.",
            plural(waited_secs as usize, "second", "seconds"),
            remaining,
            if remaining == 1 { "is" } else { "are" },
        ),
        confirm: "Force quit".into(),
    }
}

/// A protected part of the system: `id` is the group key (`plasma`, `sound`, …).
pub fn protected(id: &str, name: &str) -> Texts {
    let text = if id == "plasma" {
        "It draws your desktop, panel and windows. Ending it would close your whole session and any unsaved work, so Tidewise doesn't offer it. If your desktop seems frozen, logging out and back in is the safe fix."
    } else {
        "Your system needs it to run. Ending it could close your whole session or cut your sound or connection, so Tidewise doesn't offer it. If something seems stuck, restarting your PC is the safe fix."
    };
    Texts {
        title: format!("{name} is protected"),
        text: text.into(),
        confirm: String::new(),
    }
}

/// The toast after the app ended.
pub fn ended(name: &str, freed: u64) -> String {
    if freed == 0 {
        format!("{name} ended")
    } else {
        format!("{name} ended · {} of memory freed", format_bytes(freed))
    }
}

/// Technical details of the "not closing" dialog.
pub fn details(sent: usize, exited: usize, still_running: &[u32]) -> String {
    let pids: Vec<String> = still_running.iter().map(u32::to_string).collect();
    format!(
        "sent SIGTERM to {} · {exited} exited\nstill running: PID {} · next: SIGKILL",
        plural(sent, "process", "processes"),
        pids.join(", ")
    )
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
    fn force_quit_from_the_details_card_asks_once_and_says_nothing_is_saved() {
        let t = force_confirm("Firefox", 14);
        assert_eq!(t.title, "Force quit Firefox?");
        assert_eq!(
            t.text,
            "All 14 Firefox processes will stop immediately. Anything you haven't saved will be lost, and it gets no chance to clean up."
        );
        assert_eq!(t.confirm, "Force quit");
        assert!(force_confirm("Kdenlive", 1)
            .text
            .starts_with("Kdenlive will stop immediately."));
        assert!(force_confirm("Kdenlive", 2)
            .text
            .starts_with("Both Kdenlive processes will stop immediately."));
        assert!(
            !t.text.contains("close on its own"),
            "no polite request here"
        );
    }

    #[test]
    fn the_waiting_dialog_says_what_is_really_happening() {
        let polite = closing("Firefox", false);
        assert_eq!(polite.title, "Closing Firefox…");
        assert_eq!(
            polite.text,
            "Waiting up to 5 seconds for it to close on its own."
        );
        let kill = closing("Firefox", true);
        assert_eq!(kill.title, "Force quitting Firefox…");
        assert_eq!(kill.text, "Stopping it right away.");
        assert_eq!(kill.confirm, "");
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
