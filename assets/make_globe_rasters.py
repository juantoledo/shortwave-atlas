"""Build Shortwave Atlas globe rasters (4096x2048 grayscale JPEG, equirectangular).

usage (needs numpy + pillow), from a folder holding the two sources:
  curl -O https://naciscdn.org/naturalearth/50m/raster/SR_50M.zip && unzip SR_50M.zip
  curl -O https://eoimages.gsfc.nasa.gov/images/imagerecords/144000/144898/BlackMarble_2016_3km.jpg
  python3 make_globe_rasters.py <swatlas>/ui/src/assets/globe

relief.jpg  Natural Earth SR_50M shaded relief, re-centred so flat terrain = 128
            (neutral under a soft-light blend). Public domain, naturalearthdata.com.
lights.jpg  NASA Black Marble 2016 city lights: red channel minus the dim land/ice
            background, gamma-lifted so small towns show. Public domain, NASA EO.
"""
import sys
import numpy as np
from PIL import Image

Image.MAX_IMAGE_PIXELS = None
W, H = 4096, 2048
out = sys.argv[1]

sr = np.asarray(Image.open("SR_50M.tif"), dtype=np.float32)
rel = np.clip(128 + (sr - 206) * 0.62, 0, 255).astype(np.uint8)
Image.fromarray(rel).resize((W, H), Image.LANCZOS).save(f"{out}/relief.jpg", quality=82, optimize=True)

bm = np.asarray(Image.open("BlackMarble_2016_3km.jpg"), dtype=np.float32)
lit = np.clip((bm[..., 0] - 45) / 210, 0, 1)
small = Image.fromarray((lit * 255).astype(np.uint8)).resize((W, H), Image.BOX)
lifted = np.power(np.asarray(small, dtype=np.float32) / 255, 0.6)
Image.fromarray((lifted * 255).round().astype(np.uint8)).save(f"{out}/lights.jpg", quality=80, optimize=True)
