use sha2::{Digest, Sha256};

/// Check if a session name is valid (alphanumeric, hyphens, and underscores only)
pub fn is_valid_session_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Convert arbitrary caller-provided text into a valid session-name component.
pub fn sanitize_session_component(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;

    for c in value.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_was_sep = false;
        } else if c == '-' || c == '_' {
            if !out.is_empty() && !last_was_sep {
                out.push(c);
                last_was_sep = true;
            }
        } else if !out.is_empty() && !last_was_sep {
            out.push('-');
            last_was_sep = true;
        }
    }

    while out.ends_with(['-', '_']) {
        out.pop();
    }

    out
}

/// Convert a namespace into a collision-resistant directory component.
///
/// Already-safe, lowercase values retain their existing paths. Values that
/// require normalization use a digest of the original text so distinct
/// namespaces cannot collapse onto the same daemon or restore-state directory.
pub fn namespace_storage_component(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }

    let sanitized = sanitize_session_component(value);
    if sanitized == value {
        return sanitized;
    }

    let digest = Sha256::digest(value.as_bytes());
    format!("ns-{digest:x}")
}

/// Generate error message for invalid session name
pub fn session_name_error(name: &str) -> String {
    format!(
        "Invalid session name '{}'. Only alphanumeric characters, hyphens, and underscores are allowed.",
        name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_session_component_produces_valid_component() {
        let value = sanitize_session_component("Next Dev Loop: /Users/me/worktree!");

        assert_eq!(value, "next-dev-loop-users-me-worktree");
        assert!(is_valid_session_name(&value));
    }

    #[test]
    fn sanitize_session_component_trims_separators() {
        assert_eq!(sanitize_session_component(" --Agent__ "), "agent");
    }

    #[test]
    fn namespace_storage_component_preserves_safe_values() {
        assert_eq!(namespace_storage_component("worktree-one"), "worktree-one");
    }

    #[test]
    fn namespace_storage_component_keeps_normalization_collisions_distinct() {
        let with_space = namespace_storage_component("Worktree One");
        let with_hyphen = namespace_storage_component("Worktree-One");

        assert_ne!(with_space, with_hyphen);
        assert!(with_space.starts_with("ns-"));
        assert!(with_hyphen.starts_with("ns-"));
    }

    #[test]
    fn namespace_storage_component_does_not_drop_nonempty_values() {
        assert!(!namespace_storage_component("!!!").is_empty());
    }
}
