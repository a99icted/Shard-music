The frog variants and the headphone/grooving cat atlases are original pixel art drawn by
`scripts/draw-frog.py` using Pillow. They do not use an image-generation API.
The source is a 32×24 pixel canvas, expanded with nearest-neighbor sampling
into eight 128×96 atlas cells. The UI loads 64×48 display frames once and
reuses them. Sleep symbols and musical notes are separate UI elements.

`scripts/prepare-pets.py` prepares the existing cat, bird and dancing-cat
atlases; `scripts/recolor-bird.py` creates the feather-only bird variants.

`scripts/draw-cursor-pets.py` draws the cursor companions as eight 32×24 poses in four directions and also draws the seated piglet loops.
The UI prepares 48×36 frames for each direction once (216 KiB total).
