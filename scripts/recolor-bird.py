"""Build feather-only animation variants; retain the beak, eye and nest pixels."""
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

root = Path(__file__).resolve().parents[1]
atlas = Image.open(root / "assets/pets/bird.png").convert("RGBA")
variants = {name: Image.new("RGBA", atlas.size) for name in ["gray", "white", "black"]}
for index in range(8):
    tile = atlas.crop((index * 128, 0, (index + 1) * 128, 96))
    rgb = np.array(tile)
    # Feather crest gives a frame-specific vertical offset for the breathing cycle.
    warm = (rgb[:, :, 0] > 135) & (rgb[:, :, 1] > 65) & (rgb[:, :, 2] < 145) & (rgb[:, :, 3] > 180)
    ys, xs = np.where(warm & (np.indices(warm.shape)[0] < 50))
    dy = (int(ys.min()) if len(ys) else 29) - 29
    mask = Image.new("L", (128, 96)); draw = ImageDraw.Draw(mask)
    shape = [(36,54),(39,44),(46,36),(55,31),(61,28),(65,30),(64,32),(73,33),(79,38),(86,41),(91,48),(93,55),(98,53),(99,56),(95,62),(90,67),(78,70),(68,72),(56,72),(46,70),(40,67),(41,64),(36,61),(34,58)]
    draw.polygon([(x,y+dy) for x,y in shape],fill=255)
    # The yellow beak is a separate material, not plumage.
    draw.polygon([(39,57+dy),(44,55+dy),(48,57+dy),(48,61+dy),(44,64+dy),(39,63+dy)],fill=0)
    m = np.asarray(mask).astype(float) / 255
    source = np.asarray(tile).copy()
    r,g,b,a = [source[:,:,n].astype(float) for n in range(4)]
    feather = (r > 62) & (g > 32) & (r > b + 15) & (a > 0)
    m *= feather
    luminance = r*.30 + g*.59 + b*.11
    for name,destination in variants.items():
        result = source.copy()
        if name == "gray":
            level = np.clip(luminance*.88+18,0,235)
            channels = [level*.96,level,level*1.06]
        elif name == "white":
            level = np.clip(125+luminance*.55,0,251)
            channels = [level,level*.99,level*.96]
        else:
            level = np.clip(12+luminance*.26,0,90)
            channels = [level*.93,level,level*1.12]
        for n,target in enumerate(channels): result[:,:,n] = np.rint(source[:,:,n]*(1-m)+target*m).astype('uint8')
        destination.alpha_composite(Image.fromarray(result), (index*128,0))
for name,image in variants.items(): image.save(root / f"assets/pets/bird-{name}.png", optimize=True)
# A reviewable contact sheet, kept with build output rather than runtime assets.
preview=Image.new('RGB',(512*4,384+40),(25,31,29));draw=ImageDraw.Draw(preview)
for i,(name,image) in enumerate([('Original',atlas),*[(k,v) for k,v in variants.items()]]):
    im=image.crop((0,0,128,96)).resize((512,384),Image.Resampling.NEAREST)
    preview.paste(im,(i*512,40),im);draw.text((i*512+20,15),name,fill='white')
preview.save(root/'target/bird-variants.png')
