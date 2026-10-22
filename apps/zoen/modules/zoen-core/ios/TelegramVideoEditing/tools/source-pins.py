"""Check local adapted-source bytes separately from immutable upstream hashes."""
from pathlib import Path
import hashlib
import json


def verify_local_sources(video: Path):
    checked = []

    def verify(root, path, expected, size=None):
        target = (root / path).resolve()
        target.relative_to(root.resolve())
        data = target.read_bytes()
        assert hashlib.sha256(data).hexdigest() == expected, str(target) + ' source hash mismatch'
        assert size is None or len(data) == size, str(target) + ' source size mismatch'
        checked.append({'path': str(target), 'sha256': expected, 'bytes': len(data)})

    presentation = video.parent / 'TelegramAttachmentPresentation'
    for item in json.loads((presentation / 'UPSTREAM_SOURCES.json').read_text())['portedFiles']:
        verify(presentation, item['path'], item['sha256'], item['bytes'])
    for item in json.loads((video / 'upstream-sources.json').read_text())['sources']:
        verify(video, item['output'], item['output_sha256'])
        for fragment in item.get('local_files', []):
            verify(video, fragment['path'], fragment['sha256'], fragment['bytes'])
    return checked
