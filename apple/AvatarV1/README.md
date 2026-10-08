# AvatarV1: Zoen default agent and group pictures

Preset `v1-vivid` (style `zoen-ink-wash/1+v1-vivid`): ink and watercolour on paper, with a pastel wash behind each subject.
There are 24 agents and 22 groups. `Manifest.json` is the source of truth for names, tags, colours and files.

## Add to the app
1. Drag `AvatarV1.xcassets` into the app target (or merge its imagesets into the main catalog).
2. Add `Heroes/` and `Loops/` as **folder references** (blue folders) so the paths in `Manifest.json` resolve
   against `Bundle.main`.
3. Bundle `Manifest.json`.

## Use
- **Lists and Reduce Motion:** `Image(asset.files.still)`, a 64 pt still with 1x/2x/3x. Clip it to a circle.
  The art is circle-safe, so no subject is ever cropped.
- **Large views (profile, Space header):** `UIImage(contentsOfFile: Bundle.main.path(forResource: "Heroes/<Name>", ofType: "webp"))`.
  ImageIO decodes WebP on iOS 14+.
- **Animation:** `CGAnimateImageAtURLWithBlock` on `Loops/<Name>.webp`. It's 192 px, 3 frames at 10 fps,
  looping forever. Only animate when `accessibilityReduceMotion` is off, and only where the avatar is
  56 pt or larger.
- **Dark mode:** multiply by `darkModeMultiply` (0.86, 0.84, 0.80) so the paper doesn't glare:
  `.colorMultiply(Color(red: 0.86, green: 0.84, blue: 0.80))` when `colorScheme == .dark`.
- **Tinting:** use `colors.tint` for chips, rings and placeholders, and `colors.backdrop` to match the pastel
  wash. `dominant` and `accent` can come out paper-coloured on white subjects.

## Xcode compression
The imageset PNGs are 8-bit palette files. actool re-encodes images into `Assets.car`, so check the built
size. If it grows, set the imagesets' *Compression* to **Lossy** in the Attributes inspector.

## People
This set is for **agents** and **groups/Spaces** only. People use their own photos, and a person with no
photo gets a monogram (`design-art-generation.md` §9). Never generate faces for people.
