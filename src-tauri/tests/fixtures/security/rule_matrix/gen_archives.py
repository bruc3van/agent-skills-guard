"""Generate binary archive fixtures for security rule matrix tests."""
from pathlib import Path
import zipfile

base = Path(__file__).resolve().parent

# Path traversal ZIP
traversal_dir = base / "p2-archive/path-traversal"
traversal_dir.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(traversal_dir / "evil.zip", "w") as z:
    z.writestr("../../etc/passwd", b"root:x:0:0")
    z.writestr("safe.txt", b"ok")

# ZIP bomb (high compression ratio)
bomb_dir = base / "p2-archive/zip-bomb"
bomb_dir.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(bomb_dir / "bomb.zip", "w", compression=zipfile.ZIP_DEFLATED) as z:
    z.writestr("bomb.txt", b"A" * (2 * 1024 * 1024))

# Office OLE embedding
ole_dir = base / "p2-archive/ole-docx"
ole_dir.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(ole_dir / "macro.docx", "w") as z:
    z.writestr("word/embeddings/oleObject1.bin", b"OLE")
    z.writestr("[Content_Types].xml", '<?xml version="1.0"?><Types/>')

print("archive fixtures ok")