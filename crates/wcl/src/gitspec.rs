//! The `<rev>:<path>` argument convention shared by `wcl diff` and
//! `wcl wad spec`.
//!
//! Classifying a CLI argument is a CLI concern and stays here; reading the
//! tree at the named revision is [`wcl_wdoc::git`]'s, so a library (the
//! wskill model, which loads at a revision to diff two graphs) can do it
//! without the binary.

use std::path::{Path, PathBuf};

pub(crate) use wcl_wdoc::git::{materialize_rev, repo_rel, resolve_rev};

/// A parsed diff input: either a working-tree path or a git revision + path.
#[derive(Debug, PartialEq)]
pub(crate) enum Spec {
    Working(PathBuf),
    Git { rev: String, path: String },
}

/// Classify a diff argument. A real file on disk always wins (so a literal
/// file is never mistaken for a revision); otherwise `<rev>:<path>` with
/// non-empty halves is a git spec, guarding against a bare Windows drive
/// letter (`C:\…`). Disambiguate a colon-named file with `./name`.
pub(crate) fn parse_spec(arg: &str) -> Spec {
    if Path::new(arg).exists() {
        return Spec::Working(PathBuf::from(arg));
    }
    if let Some((rev, path)) = arg.split_once(':')
        && !rev.is_empty()
        && !path.is_empty()
        && !is_windows_drive(rev)
    {
        return Spec::Git {
            rev: rev.to_string(),
            path: path.to_string(),
        };
    }
    Spec::Working(PathBuf::from(arg))
}

/// A single ASCII letter, i.e. a Windows drive prefix like `C` in `C:\…`.
fn is_windows_drive(rev: &str) -> bool {
    let mut chars = rev.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic()) && chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_relative_path_is_working() {
        // A path that doesn't exist and has no colon stays a working spec.
        assert_eq!(
            parse_spec("nope.wcl"),
            Spec::Working(PathBuf::from("nope.wcl"))
        );
    }

    #[test]
    fn rev_path_is_a_git_spec() {
        assert_eq!(
            parse_spec("HEAD~1:config.wcl"),
            Spec::Git {
                rev: "HEAD~1".to_string(),
                path: "config.wcl".to_string()
            }
        );
        assert_eq!(
            parse_spec("main:docs/a.wcl"),
            Spec::Git {
                rev: "main".to_string(),
                path: "docs/a.wcl".to_string()
            }
        );
    }

    #[test]
    fn windows_drive_is_not_a_git_spec() {
        assert_eq!(
            parse_spec("C:\\tmp\\a.wcl"),
            Spec::Working(PathBuf::from("C:\\tmp\\a.wcl"))
        );
    }
}
