"""Author frog and headphone-cat loops with a fixed baseline.

Hand-drawn raster artwork, not an AI image-generation API. Requires Pillow.
The logical canvas is 32x24; nearest-neighbor scaling preserves exact pixels.
"""
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets/pets"
OUT.mkdir(parents=True, exist_ok=True)

INK = "#294a3d"
SHADE = "#548657"
GREEN = "#8bbf69"
LIGHT = "#b8db86"
CREAM = "#e4e9b1"
PINK = "#e6aa91"


def pose(frame, variant):
    im = Image.new("RGBA", (32, 24))
    d = ImageDraw.Draw(im)
    # A soft lily pad and planted toes define the same ground in every pose.
    d.polygon([(3, 21), (8, 20), (26, 20), (30, 22), (26, 23), (5, 23), (1, 22)], fill="#477653")
    d.line([(6, 22), (26, 22)], fill="#61945d")
    # Chubby sitting silhouette. No frame translates the frog or its feet.
    d.polygon([(7, 12), (6, 10), (6, 6), (8, 4), (12, 4), (14, 6),
               (18, 6), (20, 4), (24, 4), (26, 6), (26, 10), (25, 12),
               (27, 16), (28, 19), (26, 22), (6, 22), (4, 19), (5, 16)], fill=INK)
    d.polygon([(8, 12), (7, 10), (7, 7), (9, 5), (11, 5), (13, 7),
               (19, 7), (21, 5), (23, 5), (25, 7), (25, 10), (24, 12),
               (26, 16), (26, 20), (24, 21), (8, 21), (6, 20), (6, 16)], fill=GREEN)
    d.polygon([(7, 15), (9, 18), (9, 21), (7, 21), (5, 19)], fill=SHADE)
    d.polygon([(25, 15), (23, 18), (23, 21), (25, 21), (27, 19)], fill=SHADE)
    d.line([(8, 12), (11, 11), (21, 11), (24, 12)], fill=LIGHT)
    d.line([(9, 6), (11, 6)], fill=LIGHT)
    d.line([(21, 6), (23, 6)], fill=LIGHT)
    # Only the belly changes by one logical pixel as it breathes.
    breath = 1 if frame in (2, 3, 4, 5) else 0
    d.polygon([(12, 17 - breath), (14, 16 - breath), (19, 16 - breath),
               (21, 18), (21, 20), (19, 21), (13, 21), (11, 20), (11, 18)], fill=LIGHT)
    d.line([(14, 18 - breath), (18, 18 - breath)], fill=CREAM)
    closed = variant in ("sleep", "dream") or (variant == "blink" and frame in (3, 4))
    for x in (9, 21):
        if closed:
            d.line([(x-1, 9), (x, 10), (x+2, 10), (x+3, 9)], fill=INK)
        else:
            d.rectangle((x, 7, x+2, 10), fill=CREAM)
            d.rectangle((x+1, 8, x+2, 10), fill=INK)
            d.point((x+1, 8), fill="#f6f3da")
    d.line([(14, 12), (15, 13), (17, 13), (18, 12)], fill=INK)
    d.rectangle((8, 12, 10, 12), fill=PINK)
    d.rectangle((22, 12, 24, 12), fill=PINK)
    # Tiny forelegs and toes stay at precisely the same anchor.
    d.line([(10, 17), (10, 20), (12, 21)], fill=INK)
    d.line([(22, 17), (22, 20), (20, 21)], fill=INK)
    d.line([(7, 21), (9, 21)], fill=LIGHT)
    d.line([(23, 21), (25, 21)], fill=LIGHT)
    if variant == "breathe":
        # A contented throat puff; the torso and baseline remain unchanged.
        d.rectangle((15, 14, 17, 14 + breath), fill=CREAM)
    if variant == "dream":
        alpha = (90, 140, 200, 255, 255, 200, 140, 90)[frame]
        d.point((3, 5), fill=(238, 222, 139, alpha))
        d.line([(28, 1), (28, 3)], fill=(238, 222, 139, alpha))
        d.line([(27, 2), (29, 2)], fill=(238, 222, 139, alpha))
    return im


def headphones(im, frame):
    d = ImageDraw.Draw(im)
    d.line([(5, 9), (5, 4), (8, 1), (24, 1), (27, 4), (27, 9)], fill="#273637", width=2)
    d.line([(7, 4), (9, 2), (23, 2), (25, 4)], fill="#b0a8ca")
    for x in (3, 25):
        d.rectangle((x, 8, x+3, 14), fill="#273637")
        d.rectangle((x+1, 9, x+2, 13), fill="#b0a8ca")
        d.point((x+1, 10 if frame in (2, 3, 4, 5) else 11), fill="#ded6e7")
    return im


def cat_pose(frame):
    im = Image.new("RGBA", (32, 24))
    d = ImageDraw.Draw(im)
    dark, fur, shade, cream = "#594940", "#d6ad7b", "#ae855e", "#eee0bd"
    # A curled tail, planted paws and a softly rounded body.
    d.polygon([(4, 16), (2, 17), (2, 21), (4, 23), (11, 23), (12, 21),
               (24, 23), (27, 21), (27, 16), (25, 13), (24, 10), (25, 4),
               (23, 3), (19, 7), (13, 7), (9, 3), (7, 4), (8, 11), (7, 14)], fill=dark)
    d.polygon([(9, 5), (13, 9), (19, 9), (23, 5), (23, 11), (25, 14),
               (25, 20), (23, 22), (11, 22), (9, 20), (9, 14), (10, 11)], fill=fur)
    d.polygon([(9, 6), (12, 9), (10, 9)], fill="#dca9a0")
    d.polygon([(23, 6), (20, 9), (22, 9)], fill="#dca9a0")
    d.polygon([(14, 16), (19, 16), (22, 19), (21, 22), (12, 22), (12, 19)], fill=cream)
    d.rectangle((3, 18, 4, 20), fill=fur)
    d.line([(4, 21), (8, 21)], fill=fur)
    d.line([(10, 18), (10, 21), (13, 22)], fill=dark)
    d.line([(23, 18), (23, 21), (20, 22)], fill=dark)
    d.line([(14, 8), (14, 10)], fill=shade)
    d.line([(17, 8), (17, 10)], fill=shade)
    d.line([(20, 8), (20, 10)], fill=shade)
    for x in (11, 20):
        if frame in (3, 4):
            d.line([(x, 12), (x+2, 12)], fill=dark)
        else:
            d.rectangle((x, 11, x+1, 13), fill=dark)
            d.point((x, 11), fill=cream)
    d.point((16, 13), fill="#a26c65")
    d.line([(14, 14), (15, 15), (16, 14), (17, 15), (18, 14)], fill=dark)
    d.point((9, 14), fill=cream)
    d.point((24, 14), fill=cream)
    return headphones(im, frame)


def groove(im, frame):
    # Work on the final pixel grid. A padded head layer overlaps the neck;
    # the body/feet never rotate and the headphone band cannot hit the canvas.
    angle = (0, -4, -7, -4, 0, 4, 7, 4)[frame]
    im = im.resize((128,96), Image.Resampling.NEAREST)
    body = im.copy()
    ImageDraw.Draw(body).rectangle((0,0,127,55),fill=(0,0,0,0))
    neck_color = body.getpixel((64,64))
    ImageDraw.Draw(body).rectangle((48,48,80,68),fill=neck_color)
    head = Image.new("RGBA",(128,96))
    head.alpha_composite(im.crop((0,0,128,60)),(0,8))
    head = head.rotate(angle,resample=Image.Resampling.NEAREST,center=(64,60))
    ImageDraw.Draw(head).rectangle((0,76,127,95),fill=(0,0,0,0))
    body.alpha_composite(head)
    return body


def melancholy(frame):
    im = pose(frame, "melancholy")
    d = ImageDraw.Draw(im)
    for x in (9, 21):
        d.rectangle((x-1, 7, x+3, 10), fill=GREEN)
        d.line([(x-1, 9), (x+3, 9)], fill=INK)
        d.point((x+1, 10), fill=INK)
    d.rectangle((8, 12, 10, 12), fill=GREEN)
    d.rectangle((22, 12, 24, 12), fill=GREEN)
    d.rectangle((14, 12, 18, 14), fill=GREEN)
    d.line([(14, 14), (15, 13), (17, 13), (18, 14)], fill=INK)
    # A tiny paper cigarette and a muted ember, with smoke drawn by the UI.
    d.line([(18, 14), (25, 14)], fill="#dcd9bd")
    d.line([(19, 15), (26, 15)], fill=INK)
    d.point((25, 14), fill="#cc925c")
    d.point((26, 14), fill="#9fa49a")
    palette = {GREEN: "#8ba18a", LIGHT: "#adbaa0", SHADE: "#667d68", CREAM: "#c5cbb0", INK: "#3a4d45"}
    colors = {tuple(bytes.fromhex(a[1:])): tuple(bytes.fromhex(b[1:])) for a,b in palette.items()}
    pixels = im.load()
    for y in range(im.height):
        for x in range(im.width):
            p = pixels[x,y]
            pixels[x,y] = (*colors.get(p[:3],p[:3]),p[3])
    return im


atlases = []
for name in ("sleep", "blink", "breathe", "dream", "headphones", "grooving", "melancholy"):
    atlas = Image.new("RGBA", (1024, 96))
    for i in range(8):
        tile = pose(i, name)
        if name == "headphones":
            tile = headphones(tile, i)
        elif name == "grooving":
            tile = groove(headphones(pose(3, "sleep"), 3), i)
        elif name == "melancholy":
            tile = melancholy(i)
        tile = tile.resize((128, 96), Image.Resampling.NEAREST)
        atlas.alpha_composite(tile, (i * 128, 0))
    destination = OUT / f"frog-{name}.png"
    atlas.save(destination, optimize=True)
    atlases.append((name, atlas))
    print(f"{destination}: {destination.stat().st_size} bytes")

cat = Image.new("RGBA", (1024, 96))
for i in range(8):
    cat.alpha_composite(cat_pose(i).resize((128, 96), Image.Resampling.NEAREST), (i*128, 0))
cat.save(OUT / "cat-headphones.png", optimize=True)
atlases.append(("cat-headphones", cat))

cat_grooving = Image.new("RGBA", (1024, 96))
for i in range(8):
    tile = groove(cat_pose(3), i).resize((128, 96), Image.Resampling.NEAREST)
    cat_grooving.alpha_composite(tile, (i*128, 0))
cat_grooving.save(OUT / "cat-grooving.png", optimize=True)
atlases.append(("cat-grooving", cat_grooving))

# A review image is separate from the assets included in the executable.
review = Image.new("RGB", (192 * len(atlases), 184), "#111b17")
d = ImageDraw.Draw(review)
for i, (name, atlas) in enumerate(atlases):
    frame = atlas.crop((0, 0, 128, 96)).resize((192, 144), Image.Resampling.NEAREST)
    review.paste(frame, (i * 192, 25), frame)
    d.text((i * 192 + 16, 9), name, fill="#d4dfce")
review.save(ROOT / "target/frog-variants.png")
