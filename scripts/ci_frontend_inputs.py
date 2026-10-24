"""Reviewed inputs for the existing CI selector; no job-selection logic."""

# Reviewed Cargo roots, test commands, client coverage and embedded-resource
# recipes. Drift selects the full lane; refreshing a pin never bypasses tests.
FRONTEND_CONTRACTS = {
    # Word includes shared frontend source through its generated bootstrap.
    "crates/miniapps/resources/wordcraft.manifest.json": "be6ef15fae6216d7817979c286967123998eabb33cd4074dc8196ac1a8ebd4e1",
    "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json": "8c4b432e0bd6cacea09cff1435aca96dd96432654223292aa8cdc85970bd5731",
    "miniapps/wordcraft-ui/build-bootstrap.mjs": "944862507c8e710eb1584b80654706a6535c34003a67ef247bab27de6d0ad31c",
    "miniapps/wordcraft-ui/build-resource-manifest.mjs": "58883d2a1600baae7d61930d85f72086e0063a80e3b95a1b7c60284fdc4085db",
    "miniapps/wordcraft-ui/prepare-engine.py": "7eb5454ac8c06c3a67083f0487a3b1ae069a2310b8312d403d33bd240f107421",
    "crates/miniapps/resources/countdown.manifest.json": "e4e75fe59826cc21cc32f60e844cae5e803b3ff4b093134599319716fe1adbd7",
    "crates/miniapps/resources/deckcraft.manifest.json": "b2d23c8ae066de35b7d8b8d74816a89ee9c62ad8abbc8e2d1370a9e304d02c4c",
    "crates/miniapps/resources/excalidraw.manifest.json": "0c781ade300c8aa15102f71b12facfd6dbd541d609ccacae5a19a8701a43400a",
    "crates/miniapps/resources/gridcraft.manifest.json": "da5ff825baa1b40da321d3cc2e82528601b1736c626153fa6d9ab1d779d7c860",
    "crates/miniapps/resources/hike.manifest.json": "c0762587d7627c503e9a5f48ed0f3e1c1c45bf28fd00d97477523d159bcab5dd",
    "crates/miniapps/resources/list.manifest.json": "1d7fe1009d8f024ec75ce76a3c1906fae87725ecd3511cba08b8f1d5c1795261",
    "crates/miniapps/resources/maptap.manifest.json": "72aebc640b874a00bdc5d524a1d8f07905d2159d76d92fbaad6d73950d10d4cd",
    "crates/miniapps/resources/pet.manifest.json": "7aaad3736d5c87b5a16f3af3e9d4cb8507505ff3420470c00e07221ee6d1b626",
    "crates/miniapps/resources/poll.manifest.json": "5f44fa3e1bb71d062cf8ea9835262a6302838e60f1aada14eda564264d48fedc",
    "crates/miniapps/resources/recipe.manifest.json": "883bfd891b0c53e44c9c828175de1a793708bf687382135e3b46c07785253942",
    "crates/miniapps/resources/schedule.manifest.json": "592f54dc702801f6dfcf2ad233940e0233492677851a3f4c818da1feb008faf9",
    "miniapps/hike/manifest.json": "914ae6e337aad530a133c2abb435e70dd4fe80e15b35815aa1a667a2d00e04d1",
    "miniapps/schedule/manifest.json": "98c2f8e8e1bda22fbed394da8619fd173543a55f21472e57d3427b5d38c95a30",
    "miniapps/gridcraft-ui/prepare-engine.py": "fdf549eaa51030840973c39ee7cd2ddd29eb2084c7c000444be39bd5715bc4d5",
    "miniapps/gridcraft-ui/build-resource-manifest.mjs": "8914b2b78e8483905fc124271a05c646981641018387dca719a085812fa8a56b",
    "miniapps/deckcraft-ui/build.py": "7c7174d17f521ba4f3d307cc21f6ef511dd54b85619383333cbf3878087b2434",
    "miniapps/deckcraft-ui/build-runtime.mjs": "6af2fd64c6a5500387e2e668943dad5caffeaed6f9b12a9edc4978de4015f36f",
    "miniapps/deckcraft-ui/build-resource-manifest.mjs": "b42a717513b924557cc5bdba395424763469b2813eede6fa1790fd18c3a30b27",
    "crates/zoen-agentd/src/citizen/codex.rs": "4dd0e69d67a1fbd02e326ff1477f29a4396c986f878782ca5af9ba6cc7253bc6",
    "crates/zoen-relay/src/lib.rs": "0a85a11509ff1968e85b09abefab3b979c5e19ba797624f2fb7a42ab1deb0981",
    "Cargo.toml": "4059ed818d502e2493f51d45a356cde25deabcf34feb40f8509ba22a5e4f0d27",
    ".github/workflows/ci.yml": "fd620ffc26625762df13419673a105463878c7f65ac4a0ba446728ee86536b5f",
    ".github/workflows/expo.yml": "085e800264619bcc17f43b676a40b9ee682cb8e7dff845058b9ff4247e1bf7e7",
    "apps/zoen/package.json": "d58538007aec677d34171d4a09f883973c1c8bf9440b141d5bc72f33b2dba14b",
    "apps/zoen/src/miniapps/vendor/deckcraft-bootstrap.manifest.json": "a1e9afddbd28602267448ad1629fdbfb83bab1c3adae26bd070582efce427d00",
    "apps/zoen/src/miniapps/vendor/runtime-support.manifest.json": "7dbb32c6ff5de48a4f8027aa41175b111d305951cdf987ec079420655cc37d89",
    "apps/zoen/src/miniapps/vendor/excalidraw-ui.manifest.json": "9e58fab46099bb8ef146d5bef194bab99385095e0ce845bb3a0f442e84ae12d3",
    "miniapps/deckcraft-ui/build-bootstrap.mjs": "ecf7c02ddf67fac3f3bbb566b00940c2c5becc5a8f94060bf410a0f9b74fb57e",
    "miniapps/gridcraft-ui/build-bootstrap.mjs": "0ed258e83d06338d27393778c552a1094e694ca1a0f585c28dec11728ecc64ec",
    "tools/miniapp-build/managed-binary-bootstrap.mjs": "93196f98be23a6c1eff4db0b68cc9758dbb2906f53f94ca48595a0028dba7f55",
    "scripts/build-expo-core.mjs": "af2c2f49e2c754b6d5d8be46bf9dd09fc4460b1b8b2613bc657251a6c57c2a64",
    "scripts/build-expo-web-core.mjs": "9504ad593e1413ed9b8e36c4b79032df1336e7ad740adbac18b5e9361fa8dfd2",
}
RESOURCE_MANIFESTS = (
    "apps/zoen/src/miniapps/vendor/deckcraft-bootstrap.manifest.json",
    "apps/zoen/src/miniapps/vendor/excalidraw-ui.manifest.json",
    "apps/zoen/src/miniapps/vendor/runtime-support.manifest.json",
    "crates/miniapps/resources/countdown.manifest.json",
    "crates/miniapps/resources/deckcraft.manifest.json",
    "crates/miniapps/resources/excalidraw.manifest.json",
    "crates/miniapps/resources/gridcraft.manifest.json",
    "crates/miniapps/resources/wordcraft.manifest.json",
    "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json",
    "crates/miniapps/resources/hike.manifest.json",
    "crates/miniapps/resources/list.manifest.json",
    "crates/miniapps/resources/maptap.manifest.json",
    "crates/miniapps/resources/pet.manifest.json",
    "crates/miniapps/resources/poll.manifest.json",
    "crates/miniapps/resources/recipe.manifest.json",
    "crates/miniapps/resources/schedule.manifest.json",
    "miniapps/hike/manifest.json",
    "miniapps/schedule/manifest.json",
)

BUNDLE_MANIFESTS = (
    "apps/zoen/src/miniapps/vendor/deckcraft-bootstrap.manifest.json",
    "apps/zoen/src/miniapps/vendor/runtime-support.manifest.json",
    "apps/zoen/src/miniapps/vendor/excalidraw-ui.manifest.json",
    "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json",
)
SOURCE_BUNDLE_MANIFESTS = (
    "apps/zoen/src/miniapps/vendor/deckcraft-bootstrap.manifest.json",
    "apps/zoen/src/miniapps/vendor/runtime-support.manifest.json",
    "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json",
)
RUST_ROOTS = ("crates", "tools/uniffi-bindgen")  # Bound by the Cargo.toml pin.
# Exact pinned bodies: Codex's disabled feature name and Apple's association
# JSON key are not filesystem readers. Any change to either body disables skip.
NON_INPUT_APP_LITERALS = frozenset({
    "crates/zoen-agentd/src/citizen/codex.rs",
    "crates/zoen-relay/src/lib.rs",
})

