import os

def read_config(filename: str):
    base_dir = "/etc/myapp"
    # Safe: resolve and verify the path stays within base_dir
    real_path = os.path.realpath(os.path.join(base_dir, filename))
    if not real_path.startswith(base_dir):
        raise ValueError("Path traversal detected")
    with open(real_path, "r") as f:
        return f.read()
