"""Split changed paths; each existing validator still checks its own inputs."""
from ci_static_documents import STATIC, POINTERS

DOCUMENTS = STATIC | POINTERS


def ui_document_paths(paths, is_frontend):
    if len(paths) > 128:
        return None
    frontend = [path for path in paths if is_frontend(path)]
    documents = [path for path in paths if path in DOCUMENTS]
    if not frontend or len(frontend) + len(documents) != len(paths):
        return None
    return frontend, documents
