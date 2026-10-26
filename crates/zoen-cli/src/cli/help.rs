//! Explicit help belongs to the option/command boundary, never opaque operands.

use std::collections::BTreeSet;

/// Mirror only the existing global option prefix. Do not search message text,
/// filenames or later subcommand arguments for a token that happens to be help.
pub(super) fn requested(args: &[String]) -> bool {
    let mut command = false;
    let mut flags = BTreeSet::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => return false,
            "--help" | "-h" => return true,
            "--home" | "--timeout" | "--for" => {
                if !flags.insert(arg) || args.next().is_none() {
                    return false;
                }
            }
            "--offline" => {
                if !flags.insert(arg) {
                    return false;
                }
            }
            _ if !command && !arg.starts_with('-') => command = true,
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::requested;

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn explicit_help_precedes_home_command_and_provider_admission() {
        for args in [
            vec!["--help"],
            vec!["-h"],
            vec!["--help", "-h", "--help"],
            vec!["--home", "absent", "--help"],
            vec!["--home", "absent", "status", "--help"],
            vec!["agent-host", "--help"],
            vec!["file", "-h"],
            vec!["status", "--help", "--help"],
            vec!["status", "--home", "absent", "-h"],
            vec!["--offline", "--timeout", "15", "--for", "2", "status", "-h"],
        ] {
            assert!(requested(&argv(&args)), "{args:?}");
        }
    }

    #[test]
    fn operands_end_help_recognition_without_rewriting_any_bytes() {
        for args in [
            vec![],
            vec!["--home", "--help"],
            vec!["--home", "-h"],
            vec!["--timeout", "--help", "status"],
            vec!["--for", "--help", "status"],
            vec!["send", "chat", "--help"],
            vec!["send", "chat", "-h"],
            vec!["send", "chat", "body", "--help"],
            vec!["dm", "@person", "--help"],
            vec!["file", "add", "chat", "--help"],
            vec!["file", "get", "item", "--out", "--help"],
            vec!["profile", "set", "--bio", "--help"],
            vec!["--", "--help"],
            vec!["status", "--", "--help"],
            vec!["send", "--", "chat", "--help"],
            vec!["--home=absent", "--help"],
            vec!["--unknown", "--help"],
            vec!["--version"],
        ] {
            let args = argv(&args);
            let original = args.clone();
            assert!(!requested(&args), "{args:?}");
            assert_eq!(args, original);
        }
    }

    #[test]
    fn duplicate_or_incomplete_globals_keep_the_existing_parser_path() {
        for args in [
            vec!["--home"],
            vec!["--home", "one", "--home", "two", "--help"],
            vec!["--offline", "--offline", "--help"],
            vec!["--timeout", "1", "status", "--timeout", "2", "-h"],
            vec!["--for", "1", "--for", "2", "--help"],
        ] {
            assert!(!requested(&argv(&args)), "{args:?}");
        }
    }
}
