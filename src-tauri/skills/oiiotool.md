---
name: oiiotool
description: Image processing for EXR/HDR workflows with OpenImageIO
requires:
  bins: [oiiotool]
---
You have **oiiotool** (OpenImageIO) available for professional image processing, especially EXR/HDR:

### Common Operations
- **Convert format**: `oiiotool input.exr -o output.png`
- **Resize**: `oiiotool input.exr --resize 1920x1080 -o output.exr`
- **Colorspace convert**: `oiiotool input.exr --colorconvert "ACEScg" "sRGB" -o output.png`
- **Exposure adjust**: `oiiotool input.exr --mulc 2.0 -o brighter.exr`
- **Crop**: `oiiotool input.exr --crop 1920x1080+0+0 -o cropped.exr`
- **Info/metadata**: `oiiotool --info -v input.exr`
- **Channel select**: `oiiotool input.exr --ch R,G,B -o rgb_only.exr`
- **Composite over**: `oiiotool fg.exr bg.exr --over -o comp.exr`

### Tips
- oiiotool handles high-dynamic-range formats (EXR, HDR) natively.
- Use `--colorconvert` for proper ACES/linear/sRGB transforms.
- Supports OIIO expressions and multi-image stacks.
- For batch processing, loop over the selected files and output to new files.
