import subprocess
import unittest


class TestGitHelper(unittest.TestCase):
    """Test cases for git helper utilities."""

    def test_get_current_branch(self):
        # Safe: test helper using subprocess with explicit args
        result = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0)

    def test_check_git_installed(self):
        result = subprocess.call(["git", "--version"])
        self.assertEqual(result, 0)


if __name__ == "__main__":
    unittest.main()
