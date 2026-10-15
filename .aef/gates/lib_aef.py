"""门禁共用：读 crates/*/module.yaml（只用到 flow 列表与标量，不依赖 PyYAML）。"""
import pathlib, re

def _list(text, key):
    m = re.search(rf"^{key}:\s*\[([^\]]*)\]", text, re.M)
    return [x.strip() for x in m.group(1).split(",") if x.strip()] if m else []

def _scalar(text, key):
    m = re.search(rf"^{key}:\s*(\S+)", text, re.M)
    return m.group(1) if m else None

def modules(root):
    out = {}
    for y in sorted(pathlib.Path(root, "crates").glob("*/module.yaml")):
        t = y.read_text()
        out[y.parent.name] = {
            "dir": y.parent,
            "depends_on": _list(t, "depends_on"),
            "owns_tables": _list(t, "owns_tables"),
            "prefix": _scalar(t, "prefix"),
        }
    return out
