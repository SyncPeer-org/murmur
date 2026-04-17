//! Built-in folder templates — preset `.murmurignore` rule sets per
//! language/use case.
//!
//! Shared between `murmur-cli` (`folder create --template <name>`) and
//! `murmur-desktop` (create-folder dropdown) so both frontends ship the same
//! list without duplication.

/// Slugs of every built-in folder template. Stable across releases — adding
/// or removing a slug is a user-visible change.
pub const TEMPLATES: &[&str] = &["rust", "node", "python", "photos", "documents", "office"];

/// Human-readable description shown next to a template in the desktop
/// dropdown and in CLI `--help` output.
pub fn template_description(slug: &str) -> Option<&'static str> {
    Some(match slug {
        "rust" => "Rust projects — excludes target/, *.rs.bk, Cargo.lock.",
        "node" => "Node.js — excludes node_modules/, dist/, build/, .next/, log files.",
        "python" => "Python — excludes .venv/, __pycache__/, *.pyc, tool caches.",
        "photos" => "Photos — includes image/video files only.",
        "documents" => "Documents — includes txt/md/pdf/office/csv.",
        "office" => "Office — Microsoft / LibreOffice document formats only.",
        _ => return None,
    })
}

/// Look up ignore-file patterns for a template slug. Returns `None` for
/// unknown names so callers can surface a validation error.
pub fn template_patterns(slug: &str) -> Option<String> {
    let patterns = match slug {
        "rust" => "target/\n**/*.rs.bk\nCargo.lock\n",

        "node" => {
            "node_modules/\ndist/\nbuild/\n.next/\n.nuxt/\n.turbo/\nnpm-debug.log*\nyarn-debug.log*\nyarn-error.log*\n"
        }

        "python" => {
            ".venv/\nvenv/\n__pycache__/\n*.pyc\n*.pyo\n.pytest_cache/\n.mypy_cache/\n.ruff_cache/\n*.egg-info/\n"
        }

        "photos" => {
            "# Include only photo/video files — exclude everything else.\n*\n!*.jpg\n!*.jpeg\n!*.png\n!*.gif\n!*.heic\n!*.heif\n!*.webp\n!*.raw\n!*.dng\n!*.mp4\n!*.mov\n!*.m4v\n!*.webm\n"
        }

        "documents" => {
            "# Include only document files.\n*\n!*.txt\n!*.md\n!*.rtf\n!*.pdf\n!*.odt\n!*.ods\n!*.odp\n!*.doc\n!*.docx\n!*.xls\n!*.xlsx\n!*.ppt\n!*.pptx\n!*.csv\n"
        }

        "office" => {
            "# Include only office-document files.\n*\n!*.doc\n!*.docx\n!*.xls\n!*.xlsx\n!*.ppt\n!*.pptx\n!*.odt\n!*.ods\n!*.odp\n"
        }

        _ => return None,
    };
    Some(patterns.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_slug_has_patterns_and_description() {
        for slug in TEMPLATES {
            assert!(
                template_patterns(slug).is_some(),
                "missing patterns: {slug}"
            );
            assert!(
                template_description(slug).is_some(),
                "missing description: {slug}"
            );
        }
    }

    #[test]
    fn test_unknown_slug_returns_none() {
        assert!(template_patterns("nope").is_none());
        assert!(template_description("nope").is_none());
    }

    #[test]
    fn test_patterns_non_empty() {
        for slug in TEMPLATES {
            let p = template_patterns(slug).unwrap();
            assert!(!p.trim().is_empty(), "empty patterns for {slug}");
        }
    }

    #[test]
    fn test_rust_template_excludes_target() {
        let p = template_patterns("rust").unwrap();
        assert!(p.contains("target/"));
    }

    #[test]
    fn test_photos_template_whitelists_extensions() {
        let p = template_patterns("photos").unwrap();
        assert!(p.contains("!*.jpg"));
        assert!(p.contains("!*.mp4"));
    }
}
