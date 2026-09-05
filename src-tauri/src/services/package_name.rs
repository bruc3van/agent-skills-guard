use crate::models::PackageManager;

pub fn valid_package_name(manager: &PackageManager, name: &str) -> bool {
    fn part(s: &str) -> bool {
        !s.is_empty()
            && s.len() <= 214
            && s.as_bytes()[0].is_ascii_alphanumeric()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    }
    match manager {
        PackageManager::Npm | PackageManager::Pnpm => {
            if let Some(scoped) = name.strip_prefix('@') {
                scoped
                    .split_once('/')
                    .is_some_and(|(scope, package)| part(scope) && part(package))
            } else {
                part(name)
            }
        }
        PackageManager::Brew => {
            let parts: Vec<_> = name.split('/').collect();
            matches!(parts.len(), 1 | 3)
                && parts
                    .iter()
                    .all(|s| part(s) || s.split_once('@').is_some_and(|(a, b)| part(a) && part(b)))
        }
        PackageManager::Scoop => {
            let parts: Vec<_> = name.split('/').collect();
            parts.len() <= 2 && parts.iter().all(|s| part(s))
        }
        PackageManager::Pip | PackageManager::Choco | PackageManager::Native => part(name),
        PackageManager::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_identifiers_but_rejects_sources_and_flags() {
        for manager in [
            PackageManager::Npm,
            PackageManager::Pnpm,
            PackageManager::Pip,
            PackageManager::Brew,
            PackageManager::Scoop,
            PackageManager::Choco,
        ] {
            for bad in [
                "--flag",
                "https://example.invalid/x",
                "git+https://example.invalid/x",
                "../payload",
                "a&calc",
                "a b",
                "",
                "a\n",
            ] {
                assert!(!valid_package_name(&manager, bad), "{manager:?}: {bad}");
            }
        }
        assert!(valid_package_name(&PackageManager::Npm, "@openai/codex"));
        assert!(valid_package_name(
            &PackageManager::Brew,
            "owner/tap/tool@2"
        ));
        assert!(valid_package_name(&PackageManager::Scoop, "extras/tool"));
        assert!(valid_package_name(
            &PackageManager::Pip,
            "some_package.name"
        ));
    }
}
