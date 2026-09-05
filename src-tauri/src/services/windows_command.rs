use anyhow::{bail, Result};

pub fn validate_cmd_argument(value: &str) -> Result<()> {
    if value
        .chars()
        .any(|c| c.is_control() || "&|<>^%!?\"()".contains(c))
    {
        bail!("UNSAFE_WINDOWS_COMMAND_ARGUMENT: shell control characters are not allowed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_shell_expansion_and_operators() {
        for arg in [
            "a&echo", "%TEMP%", "!var!", "a|b", "a^b", "a\nexit", "a\"b", "(a)",
        ] {
            assert!(validate_cmd_argument(arg).is_err(), "{arg}");
        }
        assert!(validate_cmd_argument(r"C:\Users\Test User\cli.cmd").is_ok());
        assert!(validate_cmd_argument("@scope/package").is_ok());
    }
}
