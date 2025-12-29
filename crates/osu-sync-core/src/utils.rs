//! Utility functions shared across modules.

/// Sanitize a string for use as a filename by replacing invalid characters.
///
/// This function replaces the following characters with underscores:
/// `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`
///
/// It also trims leading and trailing whitespace for safety.
///
/// # Examples
///
/// ```
/// use osu_sync_core::utils::sanitize_filename;
///
/// assert_eq!(sanitize_filename("normal_name"), "normal_name");
/// assert_eq!(sanitize_filename("path/with/slashes"), "path_with_slashes");
/// assert_eq!(sanitize_filename("file:name"), "file_name");
/// assert_eq!(sanitize_filename("  spaced  "), "spaced");
/// ```
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        // Basic cases
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        assert_eq!(sanitize_filename("normal"), "normal");
        assert_eq!(sanitize_filename("good - name"), "good - name");

        // Path separators
        assert_eq!(sanitize_filename("path/with/slashes"), "path_with_slashes");
        assert_eq!(sanitize_filename("Artist\\Song"), "Artist_Song");
        assert_eq!(sanitize_filename("a/b\\c:d"), "a_b_c_d");

        // Special characters
        assert_eq!(sanitize_filename("file:name"), "file_name");
        assert_eq!(sanitize_filename("file*name?"), "file_name_");
        assert_eq!(sanitize_filename("test*file?"), "test_file_");
        assert_eq!(sanitize_filename("file<>|name"), "file___name");
        assert_eq!(sanitize_filename("\"quoted\""), "_quoted_");

        // Trimming whitespace
        assert_eq!(sanitize_filename("  spaced  "), "spaced");
        assert_eq!(sanitize_filename("  leading"), "leading");
        assert_eq!(sanitize_filename("trailing  "), "trailing");
    }
}
