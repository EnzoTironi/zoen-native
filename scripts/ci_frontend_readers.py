"""Prove evidence links and reviewed synthetic readers are not app inputs."""
import hashlib

from ci_static_documents import STATIC, validate

# The complete reader constructs its own TemporaryDirectory/root and writes a
# fake UI follow-up there. It never reads the real message route. This contract
# applies to the reader, not to a route pair; any reader drift restores Rust.
SYNTHETIC_READERS = {
    "scripts/test-android-core-contract.py": "4f3e76a2d68ac97e5cba8c8cc2722e2b620eb1fcd09af66aea023c3ef990fcb4",
}


def material_readers(git, regular_blob, base, head, readers):
    metadata = set(readers) & STATIC
    if metadata:
        # Reuse the existing evidence consumer proof at BOTH endpoints. A new
        # runtime/build consumer or changed tooling reader invalidates it.
        validate(git, base, head, sorted(metadata))
    material = set(readers) - metadata
    for reader in material & SYNTHETIC_READERS.keys():
        for endpoint in (base, head):
            if (not regular_blob(endpoint, reader)
                    or hashlib.sha256(git("show", endpoint + ":" + reader)).hexdigest()
                    != SYNTHETIC_READERS[reader]):
                raise ValueError("Synthetic frontend reader contract drift: " + reader)
        material.remove(reader)
    return material
