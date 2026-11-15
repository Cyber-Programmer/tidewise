#![no_main]

use libfuzzer_sys::fuzz_target;
use tidewise_helper::output::{
    count_not_installed, is_lock_failure, parse_print, same_transaction,
};
use tidewise_helper_protocol::{MAX_TRANSACTION, TransactionItem, is_valid_transaction_item};

// What the helper reads from pacman's output (outside data): the `--print` lines, the lock messages and the `pacman -Q`
// list. The runner decodes bytes lossily, so the parsers only ever see valid text; invalid UTF-8 is skipped here.
// Ok and Err are both fine; a panic, a hang, or a broken promise below is a bug.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(items) = parse_print(text) {
        assert!(items.len() <= MAX_TRANSACTION, "too many lines accepted");
        assert!(
            items.iter().all(is_valid_transaction_item),
            "an invalid line was accepted"
        );
        assert!(
            same_transaction(&items, &items),
            "a list differs from itself"
        );

        // Printing the items back gives text that parses to the same items.
        let again: String = items
            .iter()
            .map(|i| format!("{}/{} {}\n", i.repo, i.name, i.version))
            .collect();
        assert_eq!(
            parse_print(&again).as_ref(),
            Ok(&items),
            "did not round-trip"
        );

        // Everything listed is "installed" according to its own `name version` listing.
        let listing: String = items
            .iter()
            .map(|i| format!("{} {}\n", i.name, i.version))
            .collect();
        assert_eq!(count_not_installed(&items, &listing), 0);
    }

    // The same text as a `pacman -Q` answer or an error text: never a panic.
    let confirmed = [TransactionItem {
        repo: "extra".to_string(),
        name: "vim".to_string(),
        version: "9.1-1".to_string(),
    }];
    assert!(count_not_installed(&confirmed, text) <= 1);
    let _ = is_lock_failure(text);
});
