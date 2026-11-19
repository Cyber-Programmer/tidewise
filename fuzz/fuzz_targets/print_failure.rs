#![no_main]

use libfuzzer_sys::fuzz_target;
use tidewise_pacman::PrintFailure;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // The input is stdout and stderr in one: they are split at the first NUL byte.
    let (stdout, stderr) = text.split_once('\0').unwrap_or((text, ""));
    let Some(failure) = tidewise_pacman::parse_print_failure(stdout, stderr) else {
        return;
    };
    // A recognised failure holds at least one item, and none of its text can look like an option or hold a
    // control character: the resolver shows these words to the user.
    let clean = |text: &str| {
        !text.is_empty() && !text.starts_with('-') && !text.chars().any(char::is_control)
    };
    match failure {
        PrintFailure::NotFound(names) => {
            assert!(!names.is_empty());
            assert!(names.iter().all(|n| clean(n)));
        }
        PrintFailure::UnknownRepo(repo) => {
            assert!(!repo.is_empty() && !repo.chars().any(char::is_control));
        }
        PrintFailure::Unsatisfiable(list) => {
            assert!(!list.is_empty());
            assert!(
                list.iter()
                    .all(|u| clean(&u.dependency) && clean(&u.required_by))
            );
        }
    }
});
