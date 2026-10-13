#!/usr/bin/env python3
"""Contrast of backdrop bubble pairs over pure white and pure black.

Body text is 15px, so the floor is 4.5:1 (M56). The sky tokens stay translucent.
The bridges painted over photo, color, gradient, doodle, and scene must clear the floor
in light, dark, and increased contrast, and the two fills stay distinct.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FLOOR = 4.5
DISTINCT = 3.0
WHITE = (255, 255, 255)
BLACK = (0, 0, 0)

SWIFT = ROOT / "apple/Shared/DesignSystem/Theme.swift"
KOTLIN_PALETTE = ROOT / "android/app/src/main/java/xyz/tironi/zoen/ui/Primitives.kt"
KOTLIN_THEME = ROOT / "android/app/src/main/java/xyz/tironi/zoen/theme/Theme.kt"
CSS = ROOT / "packages/zoen-ui/src/tokens.css"
SHELL = ROOT / "packages/zoen-ui/src/shell.css"
SPACE = ROOT / "apple/Shared/Features/SpaceView.swift"
PREVIEW = ROOT / "packages/zoen-ui/preview/preview.css"


def channel(value):
    value = value / 255
    if value <= 0.04045:
        return value / 12.92
    return ((value + 0.055) / 1.055) ** 2.4


def luminance(rgb):
    red, green, blue = rgb
    return 0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)


def contrast(left, right):
    lighter = max(luminance(left), luminance(right))
    darker = min(luminance(left), luminance(right))
    return (lighter + 0.05) / (darker + 0.05)


def composite(fill, backdrop):
    red, green, blue, alpha = fill
    return tuple(
        round(alpha * channel_fill + (1 - alpha) * channel_back)
        for channel_fill, channel_back in zip((red, green, blue), backdrop)
    )


def parse_hex(text):
    hex_digits = text.lstrip("#")
    return tuple(int(hex_digits[index : index + 2], 16) for index in (0, 2, 4))


def swift_colors():
    text = SWIFT.read_text()
    assigned = {}
    for match in re.finditer(r"static let (\w+) = ([^\n]+)", text):
        assigned[match.group(1)] = match.group(2).strip()

    def resolve(name, theme, contrast_on):
        expr = assigned[name]
        adaptive = re.search(
            r'Color\.adaptive\(light: "([#A-Fa-f0-9]+)", dark: "([#A-Fa-f0-9]+)"'
            r"(?:, lightAlpha: ([0-9.]+), darkAlpha: ([0-9.]+))?"
            r"(?:, contrastLightAlpha: ([0-9.]+), contrastDarkAlpha: ([0-9.]+))?\)",
            expr,
        )
        if adaptive:
            light, dark, light_alpha, dark_alpha, contrast_light, contrast_dark = adaptive.groups()
            rgb = parse_hex(light if theme == "light" else dark)
            alpha = 1.0
            if theme == "light":
                alpha = float(contrast_light if contrast_on and contrast_light else light_alpha or 1)
            else:
                alpha = float(contrast_dark if contrast_on and contrast_dark else dark_alpha or 1)
            return (*rgb, alpha)
        plain = re.search(r'Color\(hex: "([#A-Fa-f0-9]+)"\)', expr)
        if plain:
            return (*parse_hex(plain.group(1)), 1.0)
        alias = re.fullmatch(r"(\w+)", expr)
        if alias:
            return resolve(alias.group(1), theme, contrast_on)
        raise SystemExit("unparsed swift color " + name + " = " + expr)

    return resolve, assigned


def balanced(text, marker):
    start = text.index(marker)
    start = text.index("(", start)
    depth = 0
    for index, char in enumerate(text[start:], start):
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[start + 1 : index]
    raise SystemExit("unclosed " + marker)


def kotlin_palette():
    text = KOTLIN_PALETTE.read_text()
    blocks = {}
    for theme in ("light", "dark"):
        body = balanced(text, "val " + theme + " = ZoenPalette")
        fields = {}
        for field, hex_digits, alpha in re.findall(
            r"(\w+) = argb\(0x([0-9A-Fa-f]+)(?:, ([0-9.]+)f)?\)", body
        ):
            fields[field] = (*parse_hex(hex_digits[-6:]), float(alpha) if alpha else 1.0)
        blocks[theme] = fields
    getters = {}
    theme_text = KOTLIN_THEME.read_text()
    for match in re.finditer(
        r"val ColorScheme\.(\w+): Color\s+get\(\) = palette\(\)\.(\w+)", theme_text
    ):
        getters[match.group(1)] = match.group(2)
    return blocks, getters


def css_props(block):
    return {
        name: value.strip()
        for name, value in re.findall(r"(--[\w-]+):\s*([^;]+);", block)
    }


def css_section(text, marker):
    start = text.index(marker) + len(marker)
    end = text.index("\n}", start)
    return css_props(text[start:end])


def css_blocks():
    text = CSS.read_text()
    light = css_section(text, ":root {")
    dark = css_section(text, ':root[data-theme="dark"] {')
    scheme_dark = css_section(text, ":root:not([data-theme=\"light\"]) {")
    contrast_at = text.index("@media (prefers-contrast: more)")
    contrast_light = css_section(text[contrast_at:], ":root {")
    contrast_dark = css_section(text[contrast_at:], ':root[data-theme="dark"] {')
    return light, dark, scheme_dark, contrast_light, contrast_dark


def css_color(value):
    if value is None:
        return None
    rgb = re.fullmatch(r"rgb\((\d+) (\d+) (\d+) / ([0-9.]+)\)", value)
    if rgb:
        red, green, blue, alpha = rgb.groups()
        return (int(red), int(green), int(blue), float(alpha))
    if re.fullmatch(r"#[0-9A-Fa-f]{6}", value):
        return (*parse_hex(value), 1.0)
    raise SystemExit("unparsed css color " + value)


def shell_color(value):
    value = value.strip()
    if value.startswith("#") or value.startswith("rgb("):
        return css_color(value)
    return None


def shell_blocks():
    text = SHELL.read_text()

    def block(marker):
        start = text.index(marker) + len(marker)
        end = text.index("\n}", start)
        found = {}
        for name, value in re.findall(r"(--zs-[\w-]+):\s*([^;]+);", text[start:end]):
            color = shell_color(value)
            if color is not None:
                found[name] = color
        return found

    light = block(".zs-shell {")
    dark = block(".zs-shell[data-theme='dark'] {")
    system = block(".zs-shell[data-theme='system'] {")
    return light, dark, system


def shell_theme(light, dark, theme):
    base = dict(light)
    if theme == "dark":
        base.update(dark)
    return base


def css_pair(light, dark, contrast, theme, contrast_on):
    base = dict(light)
    if theme == "dark":
        base.update(dark)
    if contrast_on:
        base.update(contrast)

    def pick(preferred, fallback):
        return css_color(base.get(preferred) or base.get(fallback))

    return {
        "user": (
            pick("--zoen-bubble-user-on-backdrop", "--zoen-bubble-user"),
            pick("--zoen-bubble-user-on-backdrop-text", "--zoen-text-primary"),
        ),
        "agent": (
            pick("--zoen-bubble-agent-on-backdrop", "--zoen-bubble-agent"),
            pick("--zoen-bubble-agent-on-backdrop-text", "--zoen-text-primary"),
        ),
    }


def typing_chip_problems():
    text = SPACE.read_text()
    start = text.index("struct WorkingRow")
    end = text.index("\nstruct TypingDots")
    body = text[start:end]
    problems = []
    if "chatBackdrop" not in body:
        problems.append("WorkingRow does not read chatBackdrop")
    pair = "backdrop ? Palette.otherBubbleOnBackdrop : Palette.otherBubble"
    if pair not in body:
        problems.append("WorkingRow does not use the message-row pair")
    if "grey" in body.lower():
        problems.append("WorkingRow still calls the chip grey")
    return problems


def shell_other_problems(swift_resolve):
    light, dark, system = shell_blocks()
    problems = []
    for theme, block in (("light", light), ("dark", dark)):
        expected = swift_resolve("otherBubble", theme, False)
        actual = block.get("--zs-bubble-other")
        if actual != expected:
            problems.append(f"{theme} --zs-bubble-other {actual} != bubbleAgent {expected}")
        contrasted = swift_resolve("otherBubble", theme, True)
        if contrasted != expected:
            problems.append(f"{theme} increased bubbleAgent changed without a shell override")
    if system.get("--zs-bubble-other") != dark.get("--zs-bubble-other"):
        problems.append("system dark --zs-bubble-other differs from the dark theme")
    preview = PREVIEW.read_text()
    row = re.search(r"\.preview-message \{([^}]+)\}", preview)
    background = ""
    if row:
        found = re.search(r"background:\s*([^;]+);", row.group(1))
        background = found.group(1).strip() if found else ""
    if background != "var(--zs-bubble-other-on-backdrop)":
        problems.append("preview row background is " + (background or "missing"))
    shell_text = SHELL.read_text()
    for chunk in re.findall(r"@media[^{]*prefers-contrast:\s*more[^{]*\{(.*?)\n\}", shell_text, re.S):
        for value in re.findall(r"--zs-bubble-other:\s*([^;]+);", chunk):
            color = shell_color(value)
            allowed = {
                swift_resolve("otherBubble", "light", True),
                swift_resolve("otherBubble", "dark", True),
            }
            if color not in allowed:
                problems.append("increased contrast --zs-bubble-other " + value.strip())
    return problems


def main():
    only = sys.argv[1] if len(sys.argv) > 1 else "all"
    if only not in ("all", "typing-chip", "shell-other"):
        print("unknown check " + only)
        return 2
    swift_resolve, _assigned = swift_colors()
    kotlin_blocks, getters = kotlin_palette()
    css_light, css_dark, css_scheme_dark, css_contrast_light, css_contrast_dark = css_blocks()
    shell_light, shell_dark, shell_system = shell_blocks()
    failed = False
    rows = []
    chip = typing_chip_problems() if only in ("all", "typing-chip") else []
    other = shell_other_problems(swift_resolve) if only in ("all", "shell-other") else []
    for problem in chip + other:
        print(problem)
        failed = True
    if only == "typing-chip":
        print("typing chip check failed" if chip else "typing chip check passed")
        return 1 if chip else 0
    if only == "shell-other":
        print("shell other check failed" if other else "shell other check passed")
        return 1 if other else 0
    bubble_names = {
        name
        for name in set(css_dark) | set(css_scheme_dark)
        if name.startswith("--zoen-bubble") or name == "--zoen-text-primary"
    }
    for name in sorted(bubble_names):
        if css_dark.get(name) != css_scheme_dark.get(name):
            print("dark css copies differ for " + name)
            failed = True

    for name in sorted(set(shell_dark) & set(shell_system)):
        if name.startswith("--zs-bubble") and shell_dark[name] != shell_system[name]:
            print("dark shell copies differ for " + name)
            failed = True

    sky = swift_resolve("bubbleUser", "light", False)
    if abs(sky[3] - 0.3) > 0.001:
        print("sky bubbleUser light alpha changed from 0.3")
        failed = True

    for theme in ("light", "dark"):
        for contrast_on in (False, True):
            variant = "increased" if contrast_on else "base"
            swift_pairs = {
                "user": (
                    swift_resolve("myBubble", theme, contrast_on),
                    swift_resolve("myBubbleText", theme, contrast_on),
                ),
                "agent": (
                    swift_resolve("otherBubbleOnBackdrop", theme, contrast_on),
                    swift_resolve("textPrimary", theme, contrast_on),
                ),
            }
            user_field = getters.get("ownMessage", "bubbleUser")
            user_text_field = getters.get("onOwnMessage", "textPrimary")
            agent_field = getters.get("otherMessage", "bubbleAgent")
            kotlin_fields = kotlin_blocks[theme]
            kotlin_pairs = {
                "user": (kotlin_fields[user_field], kotlin_fields[user_text_field]),
                "agent": (kotlin_fields[agent_field], kotlin_fields["textPrimary"]),
            }
            web_contrast = css_contrast_light if theme == "light" else css_contrast_dark
            web_pairs = css_pair(css_light, css_dark, web_contrast, theme, contrast_on)
            for role in ("user", "agent"):
                colors = {
                    "apple": swift_pairs[role],
                    "android": kotlin_pairs[role],
                    "web": web_pairs[role],
                }
                reference = colors["apple"]
                for platform, pair in colors.items():
                    if pair != reference:
                        print(
                            f"drift {platform} {theme} {variant} {role} {pair} != apple {reference}"
                        )
                        failed = True
                    fill, text = pair
                    for backdrop, backdrop_name in ((WHITE, "white"), (BLACK, "black")):
                        ratio = contrast(composite(fill, backdrop), text[:3])
                        mark = "ok" if ratio >= FLOOR else "FAIL"
                        if ratio < FLOOR:
                            failed = True
                        rows.append(
                            (
                                platform,
                                theme,
                                variant,
                                role,
                                backdrop_name,
                                ratio,
                                mark,
                            )
                        )
            user_fill = swift_pairs["user"][0]
            agent_fill = swift_pairs["agent"][0]
            for backdrop, backdrop_name in ((WHITE, "white"), (BLACK, "black")):
                ratio = contrast(composite(user_fill, backdrop), composite(agent_fill, backdrop))
                mark = "ok" if ratio >= DISTINCT else "FAIL"
                if ratio < DISTINCT:
                    failed = True
                rows.append(("apple", theme, variant, "fills", backdrop_name, ratio, mark))
            shell = shell_theme(shell_light, shell_dark, theme)
            shell_pairs = {
                "user": (shell["--zs-bubble-own"], shell["--zs-bubble-own-ink"]),
                "agent": (shell["--zs-bubble-other-on-backdrop"], shell["--zs-ink"]),
            }
            if not contrast_on and (
                shell_pairs["user"] != swift_pairs["user"]
                or shell_pairs["agent"][0] != swift_pairs["agent"][0]
            ):
                print(f"drift shell {theme} {shell_pairs} != apple user pair and agent fill")
                failed = True
            for role, pair in shell_pairs.items():
                fill, text = pair
                for backdrop, backdrop_name in ((WHITE, "white"), (BLACK, "black")):
                    ratio = contrast(composite(fill, backdrop), text[:3])
                    mark = "ok" if ratio >= FLOOR else "FAIL"
                    if ratio < FLOOR:
                        failed = True
                    rows.append(("shell", theme, variant, role, backdrop_name, ratio, mark))

    print("platform theme variant role backdrop ratio")
    for platform, theme, variant, role, backdrop_name, ratio, mark in rows:
        print(
            f"{platform} {theme} {variant} {role} {backdrop_name} {ratio:.2f} {mark}"
        )
    if failed:
        print("contrast check failed")
        return 1
    print("contrast check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
