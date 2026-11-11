//! Shared logic for Tidewise: no UI, no root.

pub mod contrast;

/// The app's name as shown to users.
pub fn app_name() -> &'static str {
    "Tidewise"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_tidewise() {
        assert_eq!(app_name(), "Tidewise");
    }
}
