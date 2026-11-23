"""Hand-drawn pixel companions: eight poses in four directions, using Pillow."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]


def kitten(frame):
    im = Image.new("RGBA", (32, 24))
    d = ImageDraw.Draw(im)
    outline, fur, shade, light = "#634b42", "#d9b084", "#b88b65", "#f0ddba"
    stride = (0, 1, 2, 1, 0, -1, -2, -1)[frame]
    d.line([(8,16),(4,15),(2,12),(2,9+stride//2),(4,8+stride//2)], fill=outline, width=3)
    d.line([(7,16),(4,14),(3,12),(3,10)], fill=fur)
    for x, offset in ((11, -stride), (21, stride)):
        d.polygon([(x-2,17),(x+1,17),(x+offset+2,22),(x+offset+2,23),(x+offset-2,23)], fill=outline)
        d.line([(x,18),(x+offset,22)], fill=fur, width=2)
        d.point((x+offset+1,22), fill=light)
    d.ellipse((6,11,25,21), fill=outline)
    d.ellipse((7,12,24,20), fill=fur)
    d.line([(10,13),(11,15)], fill=shade, width=2)
    d.line([(14,12),(15,14)], fill=shade)
    d.polygon([(17,10),(17,4),(19,3),(23,7),(27,4),(28,9),(30,11),
               (31,14),(29,18),(23,19),(18,17)], fill=outline)
    d.polygon([(18,10),(18,5),(22,9),(26,6),(27,10),(29,12),
               (30,14),(28,17),(23,18),(19,16)], fill=fur)
    d.polygon([(19,6),(21,9),(19,9)], fill="#d99f98")
    d.point((26,8), fill="#d99f98")
    d.rectangle((26,11,27,13), fill=outline)
    d.point((26,11), fill=light)
    d.point((30,14), fill="#aa756f")
    d.line([(27,15),(28,16),(29,15)], fill=outline)
    d.line([(20,16),(22,16)], fill=light)
    return im


def frog(frame):
    im = Image.new("RGBA", (32, 24))
    d = ImageDraw.Draw(im)
    dark, green, shade, light = "#365446", "#90bb72", "#65945d", "#d0dfa2"
    stride = (0, 1, 2, 1, 0, -1, -2, -1)[frame]
    lift = (0, 0, -1, -2, -1, 0, 0, 0)[frame]
    d.polygon([(10,16),(5,17),(4,20),(8+stride,23),(14+stride,23),(15,21)], fill=dark)
    d.line([(8,18),(6,20),(10+stride,22),(14+stride,22)], fill=shade, width=2)
    d.polygon([(22,16),(25,17),(26-stride,22),(30-stride,23),(24-stride,23),(21,20)], fill=dark)
    d.line([(24,18),(25-stride,22),(28-stride,22)], fill=green)
    d.ellipse((6,9+lift,28,21+lift), fill=dark)
    d.ellipse((7,10+lift,27,20+lift), fill=green)
    d.ellipse((13,14+lift,26,20+lift), fill=light)
    d.ellipse((15,5+lift,22,12+lift), fill=dark)
    d.ellipse((16,6+lift,21,11+lift), fill=shade)
    d.ellipse((20,3+lift,29,13+lift), fill=dark)
    d.ellipse((21,4+lift,28,12+lift), fill=green)
    d.rectangle((25,6+lift,27,9+lift), fill=light)
    d.rectangle((26,7+lift,27,9+lift), fill=dark)
    d.point((26,7+lift), fill="#f4eed0")
    d.line([(24,13+lift),(25,14+lift),(28,13+lift)], fill=dark)
    d.line([(23,12+lift),(24,12+lift)], fill="#d7a68c")
    return im


def kitten_vertical(frame, back=False):
    im = Image.new("RGBA", (32, 24))
    d = ImageDraw.Draw(im)
    dark, fur, shade, light = "#634b42", "#d9b084", "#b88b65", "#f0ddba"
    stride = (0, 1, 2, 1, 0, -1, -2, -1)[frame]
    bob = (0, 0, -1, -1, 0, 0, -1, -1)[frame]
    # Four alternating paws, slight shoulder bounce and a swaying tail.
    for x, phase in ((10, stride), (21, -stride)):
        d.rounded_rectangle((x-2, 16, x+2, 22-abs(phase)), radius=1, fill=dark)
        d.rectangle((x-1, 17, x+1, 21-abs(phase)), fill=fur)
        d.line([(x-1,21-abs(phase)),(x+1,21-abs(phase))],fill=light)
    d.ellipse((7, 9+bob, 25, 20+bob), fill=dark)
    d.ellipse((8, 10+bob, 24, 19+bob), fill=fur)
    if not back:
        d.ellipse((12, 14+bob, 20, 19+bob), fill=light)
    d.polygon([(6,10+bob),(7,2+bob),(10,2+bob),(14,6+bob),(19,6+bob),
               (23,2+bob),(25,2+bob),(26,11+bob),(24,15+bob),(9,15+bob)],fill=dark)
    d.polygon([(7,10+bob),(8,4+bob),(13,8+bob),(20,8+bob),(24,4+bob),
               (25,11+bob),(23,14+bob),(10,14+bob)],fill=fur)
    d.line([(14,7+bob),(14,9+bob)],fill=shade)
    d.line([(18,7+bob),(18,9+bob)],fill=shade)
    if back:
        d.line([(16,18),(16+stride,14),(18+stride,11)],fill=dark,width=3)
        d.line([(16,18),(16+stride,14),(18+stride,11)],fill=fur)
        d.line([(10,10+bob),(11,12+bob)],fill=shade)
        d.line([(23,10+bob),(22,12+bob)],fill=shade)
    else:
        d.polygon([(8,5+bob),(11,8+bob),(9,8+bob)],fill="#d99f98")
        d.polygon([(23,5+bob),(21,8+bob),(23,8+bob)],fill="#d99f98")
        for x in (11,21):
            d.rectangle((x,10+bob,x+1,12+bob),fill=dark)
            d.point((x,10+bob),fill=light)
        d.point((16,12+bob),fill="#aa756f")
        d.line([(14,13+bob),(15,14+bob),(16,13+bob),(17,14+bob),(18,13+bob)],fill=dark)
    return im


def frog_vertical(frame, back=False):
    im = Image.new("RGBA", (32,24))
    d = ImageDraw.Draw(im)
    dark, green, shade, light = "#365446", "#90bb72", "#65945d", "#d0dfa2"
    reach = (0,1,2,3,2,1,0,0)[frame]
    bob = (0,0,-1,-2,-2,-1,0,0)[frame]
    # Hind legs tuck during the hop and extend again on landing.
    for flip in (False,True):
        pts=[(9,14),(4+reach,17),(3+reach,21),(11,22)]
        pts=[(31-x if flip else x,y) for x,y in pts]
        d.polygon(pts,fill=dark)
        x=23 if flip else 5
        d.line([(x,18),(x+(1 if flip else -1),20),(x+(3 if flip else -1),21)],fill=shade,width=2)
    d.ellipse((6,7+bob,26,21+bob),fill=dark)
    d.ellipse((7,8+bob,25,20+bob),fill=green)
    for x in (7,19):
        d.ellipse((x,3+bob,x+7,11+bob),fill=dark)
        d.ellipse((x+1,4+bob,x+6,10+bob),fill=green)
        if not back:
            d.rectangle((x+2,5+bob,x+4,8+bob),fill=light)
            d.rectangle((x+3,6+bob,x+4,8+bob),fill=dark)
            d.point((x+3,6+bob),fill="#f4eed0")
    if back:
        d.line([(16,10+bob),(16,17+bob)],fill=shade)
        for x,y in ((10,12),(21,12),(11,17),(21,17)):
            d.rectangle((x,y+bob,x+1,y+1+bob),fill=shade)
    else:
        d.ellipse((11,13+bob,22,19+bob),fill=light)
        d.line([(13,11+bob),(15,12+bob),(18,12+bob),(20,11+bob)],fill=dark)
        d.point((9,11+bob),fill="#d7a68c")
        d.point((23,11+bob),fill="#d7a68c")
        d.line([(10,16+bob),(10,20),(12,21)],fill=dark)
        d.line([(23,16+bob),(23,20),(21,21)],fill=dark)
    return im


def pig(frame):
    im=Image.new("RGBA",(32,24));d=ImageDraw.Draw(im)
    dark,pink,shade,light="#875965","#efb3be","#cf8e9f","#ffdae0"
    stride=(0,1,2,1,0,-1,-2,-1)[frame]
    bob=(0,0,-1,-1,0,0,-1,-1)[frame]
    d.line([(7,14),(3,12),(1,13),(1,15),(3,16),(4,14)],fill=dark)
    for x,phase in ((9,stride),(15,-stride),(21,-stride),(25,stride)):
        d.rectangle((x+phase,17,x+phase+2,22-abs(phase)//2),fill=dark)
        d.line([(x+phase+1,17),(x+phase+1,20-abs(phase)//2)],fill=pink)
    d.ellipse((5,8+bob,27,20+bob),fill=dark)
    d.ellipse((6,9+bob,26,19+bob),fill=pink)
    d.line([(10,11+bob),(15,10+bob)],fill=light,width=2)
    d.polygon([(19,11+bob),(17,5+bob),(20,4+bob),(24,7+bob),(27,6+bob),(29,11+bob),(29,17+bob),(22,19+bob)],fill=dark)
    d.polygon([(20,11+bob),(19,6+bob),(23,8+bob),(26,7+bob),(28,12+bob),(28,16+bob),(23,18+bob)],fill=pink)
    d.line([(20,7+bob),(22,9+bob)],fill=shade)
    d.rectangle((26,10+bob,27,12+bob),fill=dark)
    d.point((26,10+bob),fill=light)
    d.rounded_rectangle((26,13+bob,31,17+bob),radius=1,fill=shade)
    d.line([(27,14+bob),(30,14+bob)],fill=light)
    d.point((29,16+bob),fill=dark)
    return im


def pig_vertical(frame, back=False, seated=False, sleeping=False):
    im=Image.new("RGBA",(32,24));d=ImageDraw.Draw(im)
    dark,pink,shade,light="#875965","#efb3be","#cf8e9f","#ffdae0"
    stride=0 if seated else (0,1,2,1,0,-1,-2,-1)[frame]
    bob=0 if seated else (0,0,-1,-1,0,0,-1,-1)[frame]
    for x,phase in ((9,stride),(22,-stride)):
        d.rounded_rectangle((x-2,17,x+2,23-abs(phase)),radius=1,fill=dark)
        d.rectangle((x-1,18,x+1,21-abs(phase)),fill=pink)
    d.ellipse((6,10+bob,26,22),fill=dark)
    d.ellipse((7,11+bob,25,21),fill=pink)
    if seated:
        breath=int(frame in (2,3,4,5))
        d.ellipse((11,14-breath,22,20),fill=light)
    d.polygon([(6,9+bob),(4,3+bob),(7,2+bob),(12,5+bob),(20,5+bob),
               (25,2+bob),(28,3+bob),(26,9+bob),(26,14+bob),(22,17+bob),(10,17+bob),(6,14+bob)],fill=dark)
    d.polygon([(7,9+bob),(6,4+bob),(12,7+bob),(20,7+bob),(26,4+bob),(25,9+bob),
               (25,13+bob),(21,16+bob),(11,16+bob),(7,13+bob)],fill=pink)
    d.line([(7,5+bob),(10,8+bob)],fill=shade,width=2)
    d.line([(25,5+bob),(22,8+bob)],fill=shade,width=2)
    if back:
        d.line([(17,19),(20+stride//2,18),(20+stride//2,16),(17,16),(17,18)],fill=dark)
        d.line([(12,9+bob),(20,9+bob)],fill=light)
    else:
        for x in (10,21):
            if sleeping or (seated and frame in (3,4)):
                d.line([(x-1,10+bob),(x,11+bob),(x+2,10+bob)],fill=dark)
            else:
                d.rectangle((x,9+bob,x+1,11+bob),fill=dark)
                d.point((x,9+bob),fill=light)
        d.rounded_rectangle((12,11+bob,20,15+bob),radius=2,fill=shade)
        d.line([(14,12+bob),(18,12+bob)],fill=light)
        d.point((14,14+bob),fill=dark);d.point((18,14+bob),fill=dark)
        d.point((9,13+bob),fill=light);d.point((23,13+bob),fill=light)
        if seated:
            d.line([(10,17),(10,20),(12,21)],fill=dark)
            d.line([(23,17),(23,20),(21,21)],fill=dark)
    return im


review=Image.new("RGB",(8*96,3*4*86),"#18221e")
for pet_index,(name, side, vertical) in enumerate((("kitten",kitten,kitten_vertical),("frog",frog,frog_vertical),("pig",pig,pig_vertical))):
    atlas=Image.new("RGBA",(32*8,24*4))
    for frame in range(8):
        for direction,tile in enumerate((side(frame),side(frame).transpose(Image.Transpose.FLIP_LEFT_RIGHT),vertical(frame),vertical(frame,True))):
            atlas.alpha_composite(tile,(frame*32,direction*24))
            tile=tile.resize((96,72),Image.Resampling.NEAREST)
            review.paste(tile,(frame*96,(pet_index*4+direction)*86+12),tile)
    path=root/f"assets/pets/cursor-{name}.png"
    atlas.save(path,optimize=True)
    print(f"{path}: {path.stat().st_size} bytes")
review.save(root/"target/cursor-directions.png")
for sleeping in (True,False):
    atlas=Image.new("RGBA",(1024,96))
    for frame in range(8):
        tile=pig_vertical(frame,seated=True,sleeping=sleeping).resize((128,96),Image.Resampling.NEAREST)
        atlas.alpha_composite(tile,(frame*128,0))
    path=root/f"assets/pets/pig-{'sleep' if sleeping else 'blink'}.png"
    atlas.save(path,optimize=True)
    print(f"{path}: {path.stat().st_size} bytes")
