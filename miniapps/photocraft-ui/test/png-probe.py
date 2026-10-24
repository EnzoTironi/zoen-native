"""Independent bounded PNG decoder for actual Photo exports, never source authority."""
import json
import sys
from PIL import Image
Image.MAX_IMAGE_PIXELS = 1024 * 1024
if len(sys.argv) == 4 and sys.argv[1] == "rotate-clockwise":
    def bounded(path):
        with Image.open(path) as image:
            if image.format != "PNG" or not (0 < image.width <= 1024 and 0 < image.height <= 1024):
                raise ValueError("PNG outside managed canvas bounds")
            return image.convert("RGBA")
    before, after = bounded(sys.argv[2]), bounded(sys.argv[3])
    expected = before.transpose(Image.Transpose.ROTATE_270)
    if expected.size != after.size or expected.tobytes() != after.tobytes():
        raise ValueError("Actual exported pixels differ from independent clockwise rotation")
    print(json.dumps({"rotation": "clockwise90", "width": after.width,
                      "height": after.height, "allPixelsEqual": True}))
    raise SystemExit(0)
with Image.open(sys.argv[1]) as image:
    if image.format != "PNG" or not (0 < image.width <= 1024 and 0 < image.height <= 1024):
        raise ValueError("PNG outside managed canvas bounds")
    rgba = image.convert("RGBA")
    black = [(x, y) for y in range(image.height) for x in range(image.width)
             if rgba.getpixel((x, y)) == (0, 0, 0, 255)]
    print(json.dumps({"width": image.width, "height": image.height,
                     "blackPixels": len(black),
                     "newBlackPixels": sum(not (100 <= x < 160 and 90 <= y < 120) for x, y in black),
                     "white": rgba.getpixel((0, 0)),
                     "black": rgba.getpixel((100, 90)),
                     "green": rgba.getpixel((420, 310))}))
