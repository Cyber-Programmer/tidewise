//! Parser for `pacman -Sg <group>`: the members of a package group (WBS 6.1).

use crate::ParseError;

/// One line of `-Sg`: a package that belongs to a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMember {
    pub group: String,
    pub name: String,
}

/// Parses the output of `LC_ALL=C pacman -Sg <group>`: one `group member` line per member, in pacman's order.
///
/// Empty text means no members. The first bad line (counted from 1) is an error.
pub fn parse_sg(text: &str) -> Result<Vec<GroupMember>, ParseError> {
    // Red-phase shim (6.1): refuses everything. The real parser comes in the next commit.
    let _ = text;
    Err(ParseError::BadGroupLine {
        line_number: 0,
        line: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLASMA: &str = include_str!("../../../tests/fixtures/pacman/sg-plasma.txt");
    const BROKEN: &str = include_str!("../../../tests/fixtures/pacman/sg-broken.txt");

    fn member(group: &str, name: &str) -> GroupMember {
        GroupMember {
            group: group.to_string(),
            name: name.to_string(),
        }
    }

    fn bad_line(text: &str) -> (usize, String) {
        match parse_sg(text) {
            Err(ParseError::BadGroupLine { line_number, line }) => (line_number, line),
            other => panic!("expected BadGroupLine, got {other:?}"),
        }
    }

    #[test]
    fn the_plasma_group_has_68_members_in_pacmans_order() {
        let members = parse_sg(PLASMA).unwrap();
        assert_eq!(members.len(), 68);
        assert_eq!(members[0], member("plasma", "aurorae"));
        assert_eq!(members[1], member("plasma", "bluedevil"));
        assert!(members.iter().all(|m| m.group == "plasma"));
    }

    #[test]
    fn empty_output_is_no_members() {
        assert_eq!(parse_sg("").unwrap(), Vec::new());
    }

    #[test]
    fn the_final_newline_is_optional() {
        assert_eq!(
            parse_sg("plasma breeze").unwrap(),
            parse_sg("plasma breeze\n").unwrap()
        );
        assert_eq!(parse_sg("plasma breeze").unwrap().len(), 1);
    }

    #[test]
    fn the_broken_fixture_is_an_error_on_its_first_bad_line() {
        assert_eq!(bad_line(BROKEN), (3, "plasma-no-member-name".to_string()));
    }

    #[test]
    fn two_spaces_extra_words_blank_lines_and_carriage_returns_are_errors() {
        assert_eq!(bad_line("plasma  breeze\n").0, 1);
        assert_eq!(bad_line("plasma breeze extra\n").0, 1);
        assert_eq!(bad_line("plasma\n").0, 1);
        assert_eq!(bad_line("plasma breeze\n\nplasma bc\n").0, 2);
        assert_eq!(bad_line("plasma breeze\r\n").0, 1);
        assert_eq!(bad_line(" plasma breeze\n").0, 1);
        assert_eq!(bad_line("plasma breeze \n").0, 1);
        assert_eq!(bad_line("plasma\tbreeze\n").0, 1);
    }

    #[test]
    fn bad_group_or_member_names_are_errors() {
        assert_eq!(bad_line("--force breeze\n").0, 1);
        assert_eq!(bad_line("plasma --force\n").0, 1);
        assert_eq!(bad_line("plasma a/b\n").0, 1);
        assert_eq!(bad_line("pla/sma breeze\n").0, 1);
        assert_eq!(bad_line("plasma .hidden\n").0, 1);
    }

    #[test]
    fn a_member_listed_twice_is_an_error() {
        // The same line twice would make the member picker show it twice.
        let error = parse_sg("plasma breeze\nplasma breeze\n").unwrap_err();
        assert_eq!(
            error,
            ParseError::BadGroupLine {
                line_number: 2,
                line: "plasma breeze".to_string()
            }
        );
    }
}
