---
name: imagemagick
description: Image conversion, resizing, and batch processing with ImageMagick
requires:
  anyBins: [magick, convert]
---
You have **ImageMagick** available for image processing:

### Common Operations (v7 uses `magick`, v6 uses `convert`)
- **Convert format**: `magick input.png output.jpg`
- **Resize**: `magick input.png -resize 800x600 output.png`
- **Thumbnail**: `magick input.png -thumbnail 200x200^ -gravity center -extent 200x200 thumb.png`
- **Batch convert**: `magick mogrify -format jpg -path ./output *.png`
- **Compress JPEG**: `magick input.jpg -quality 80 output.jpg`
- **Strip metadata**: `magick input.jpg -strip output.jpg`
- **Add watermark**: `magick input.png -gravity southeast -annotate +10+10 "Watermark" output.png`
- **Montage/grid**: `magick montage *.jpg -geometry +2+2 -tile 3x grid.jpg`
- **Info**: `magick identify -verbose input.png`

### Tips
- On systems with v7, use `magick` command. On v6, use `convert`.
- Use `mogrify` (with `-path`) for in-place batch ops (always specify output path!).
- For batch ops, use PowerShell `foreach` with `magick` per file.
- Always output to new files to protect originals.
