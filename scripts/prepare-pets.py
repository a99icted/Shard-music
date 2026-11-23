"""Build tiny transparent 8-frame atlases from the supplied 4x2 sprite sheets.

Usage: python scripts/prepare-pets.py cat-sheet.png bird-sheet.png dancing-sheet.png
Requires Pillow and numpy. No source sheets are needed at runtime.
"""
from pathlib import Path
import sys
import math
import numpy as np
from PIL import Image


def prepare(source: str, destination: Path) -> None:
    sheet = Image.open(source).convert("RGBA")
    frames = []
    for row in range(2):
        for column in range(4):
            cell = sheet.crop((column * sheet.width // 4, row * sheet.height // 2,
                               (column + 1) * sheet.width // 4, (row + 1) * sheet.height // 2))
            pixels = np.array(cell)
            rgb = pixels[:, :, :3].astype(np.int16)
            # Remove only the saturated magenta matte; retain pink ears and beaks.
            matte = (rgb[:, :, 0] > 170) & (rgb[:, :, 2] > 150) & (rgb[:, :, 1] < 100)
            pixels[matte, 3] = 0
            pixels[matte, :3] = 0
            cell = Image.fromarray(pixels)
            frames.append(cell)
    # Input sheets contain small camera shifts. Crop each pose independently,
    # use one scale for the sequence, and lock its feet/nest to the same baseline.
    boxes = [frame.getbbox() for frame in frames]
    max_width = max(box[2]-box[0] for box in boxes)
    max_height = max(box[3]-box[1] for box in boxes)
    scale = min(120/max_width, 80/max_height)
    atlas = Image.new("RGBA", (128 * 8, 96))
    for index, (frame, bounds) in enumerate(zip(frames, boxes)):
        frame = frame.crop(bounds)
        frame = frame.resize((round(frame.width*scale),round(frame.height*scale)),Image.Resampling.LANCZOS)
        pixels = np.array(frame)
        r,g,b,a = [pixels[:,:,n].astype(np.int16) for n in range(4)]
        # Remove magenta spill in antialiased outlines, including opaque purple
        # contour pixels. Warm fur, beaks, leaves and pink interior details survive.
        spill = (r > g+8) & (b > g+8) & (b > 35)
        neutral = np.minimum(g,65)
        for channel in range(3): pixels[:,:,channel] = np.where(spill,neutral,pixels[:,:,channel])
        # Tiny disconnected matte flecks must not define the visual baseline.
        frame = Image.fromarray(pixels)
        tile = Image.new("RGBA", (128, 96))
        tile.alpha_composite(frame, ((128 - frame.width) // 2, 96 - frame.height))
        atlas.alpha_composite(tile, (index * 128, 0))
    if destination.stem in {"cat", "bird"}:
        # Sleeping needs a closed, subtle breathing loop. The supplied AI sheet
        # changes body proportions between cells, which reads as a camera jump.
        # Derive all eight breathing poses from the same cleaned silhouette.
        base = atlas.crop((0,0,128,96))
        box = base.getbbox()
        subject = base.crop(box)
        atlas = Image.new("RGBA", (1024,96))
        for i in range(8):
            breath = (1-math.cos(2*math.pi*i/8))/2
            height = subject.height + round(breath*1.5)
            pose = subject.resize((subject.width,height),Image.Resampling.BICUBIC)
            atlas.alpha_composite(pose,(i*128+(128-pose.width)//2,96-height))
    destination.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(destination, optimize=True)
    print(f"{destination}: {destination.stat().st_size} bytes, {atlas.width * atlas.height * 4} decoded bytes")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    output = Path(__file__).resolve().parents[1] / "assets" / "pets"
    for source, name in zip(sys.argv[1:], ["cat", "bird", "dancing-cats"]):
        prepare(source, output / f"{name}.png")
