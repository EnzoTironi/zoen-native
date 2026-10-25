"""Reviewed inputs for the existing CI selector; no job-selection logic."""

# Reviewed Cargo roots, test commands, client coverage and embedded-resource
# recipes. Drift selects the full lane; refreshing a pin never bypasses tests.
FRONTEND_CONTRACTS = {
    "scripts/ci-rust-shards.py": "66cf8a228cb51e4f38ba4c23be42f88a43bf4a9b39daf149b0d410d813ba19ea",
    "scripts/expo-cargo-artifacts.mjs": "cebc6836153781bb5b172f1f80be5f401d1bc4a45c1429e22d6f6f20500b99dd",
    "crates/miniapps/resources/photocraft.manifest.json": "5779ad9cfbde99f8fd32c2421c3b41797216b996f708a1b26092e1e589d4792a",
    "crates/miniapps/resources/generated/photocraft-bootstrap.manifest.json": "8701151efcd1fd426676522b918fd2cba771776afc360dc2b45d242d7b0e628f",
    "miniapps/photocraft-ui/build-bootstrap.mjs": "82497408552cb6b06f47e43f9dd0f7633bf4b04ecbfe76ebdd0ae75fc5b4c78a",
    "miniapps/photocraft-ui/build-resource-manifest.mjs": "8c810ac8097bd7b42ff50b2e3f46f5bb650c90c0ff452ba92b3a431e1b847620",
    "miniapps/photocraft-ui/prepare-engine.py": "f8214583ce6f495f8a4ebff67edc808e07ab0456627601f05822f5d9f4c99cac",
    "tools/miniapp-build/binary-resource-manifest.mjs": "17f2ddd1bcf9b77fa6ff1eac47a1e608a7e797a32cc2c81208ab3166ab09e490",
    # Word includes shared frontend source through its generated bootstrap.
    "crates/miniapps/resources/wordcraft.manifest.json": "7b0c89ebcc54a64f2f178f270fe3c8ade755daa8466e8bc0c2f21242dc6fd586",
    "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json": "3b50cb777372f74e463fdb8c19b359eb8f774514efc86f6dd66ca18f6b72dc49",
    "miniapps/wordcraft-ui/build-bootstrap.mjs": "944862507c8e710eb1584b80654706a6535c34003a67ef247bab27de6d0ad31c",
    "miniapps/wordcraft-ui/build-resource-manifest.mjs": "58883d2a1600baae7d61930d85f72086e0063a80e3b95a1b7c60284fdc4085db",
    "miniapps/wordcraft-ui/prepare-engine.py": "a0233d12d280a9a8bf502b278dd0645374c54ccb65aedee60f17bcd310721988",
    "crates/miniapps/resources/countdown.manifest.json": "e4e75fe59826cc21cc32f60e844cae5e803b3ff4b093134599319716fe1adbd7",
    "crates/miniapps/resources/deckcraft.manifest.json": "cb539d4fec1b4a0c465ed2fe655dad17b2f2f7f1a7658f6535c9891859134716",
    "crates/miniapps/resources/excalidraw.manifest.json": "0c781ade300c8aa15102f71b12facfd6dbd541d609ccacae5a19a8701a43400a",
    "crates/miniapps/resources/gridcraft.manifest.json": "c85db481b36a56c95e61e437e0e943c97a5bef1b0bae2ae3438edfca82401236",
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
    "Cargo.toml": "a3fef06b8397655b0b3a187fa53889c5c13f92fb0778e4a1f765c8920d9f434c",
    ".github/workflows/ci.yml": "7f022560bb9ba28bfda8db147ab1a62e403eba5b12bd30b4588f207b4ecee9f6",
    ".github/workflows/expo.yml": "085e800264619bcc17f43b676a40b9ee682cb8e7dff845058b9ff4247e1bf7e7",
    "apps/zoen/package.json": "d58538007aec677d34171d4a09f883973c1c8bf9440b141d5bc72f33b2dba14b",
    "apps/zoen/src/miniapps/vendor/deckcraft-bootstrap.manifest.json": "3743f0aa88f999d061bc762559f063f4044d45a19634469e373c35f309bf4c64",
    "apps/zoen/src/miniapps/vendor/runtime-support.manifest.json": "32247f230b0f7506f35c97d374f44a923b6b99d46d95814d89707cbf9eb366a0",
    "apps/zoen/src/miniapps/vendor/excalidraw-ui.manifest.json": "9e58fab46099bb8ef146d5bef194bab99385095e0ce845bb3a0f442e84ae12d3",
    "miniapps/deckcraft-ui/build-bootstrap.mjs": "ecf7c02ddf67fac3f3bbb566b00940c2c5becc5a8f94060bf410a0f9b74fb57e",
    "miniapps/gridcraft-ui/build-bootstrap.mjs": "0ed258e83d06338d27393778c552a1094e694ca1a0f585c28dec11728ecc64ec",
    "tools/miniapp-build/managed-binary-bootstrap.mjs": "a6672dff3cd8b2c3d08545e122d5cdddf924756d3bdad4e364cbb7b28f57e3bb",
    "scripts/build-expo-core.mjs": "99fa3265955b75ce881f413b4ef93832c1010aa948fd3e7b225e95ae1b9aef96",
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
    "crates/miniapps/resources/photocraft.manifest.json",
    "crates/miniapps/resources/generated/photocraft-bootstrap.manifest.json",
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
    "crates/miniapps/resources/generated/photocraft-bootstrap.manifest.json",
)
SOURCE_BUNDLE_MANIFESTS = (
    "apps/zoen/src/miniapps/vendor/deckcraft-bootstrap.manifest.json",
    "apps/zoen/src/miniapps/vendor/runtime-support.manifest.json",
    "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json",
    "crates/miniapps/resources/generated/photocraft-bootstrap.manifest.json",
)
RUST_ROOTS = ("crates", "tools/uniffi-bindgen")  # Bound by the Cargo.toml pin.
# Exact pinned bodies: Codex's disabled feature name and Apple's association
# JSON key are not filesystem readers. Any change to either body disables skip.
NON_INPUT_APP_LITERALS = frozenset({
    "crates/zoen-agentd/src/citizen/codex.rs",
    "crates/zoen-relay/src/lib.rs",
})

