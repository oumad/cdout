---
name: exiftool
description: Read and write metadata for images, RAW, video, and EXR files
requires:
  bins: [exiftool]
---
You have **ExifTool** available for reading and writing metadata across stills, RAW, video, and even EXR files. Use it for:

### Common Operations
- **Read all metadata**: `exiftool input.jpg`
- **Read specific tag**: `exiftool -CreateDate -Make -Model input.cr3`
- **JSON output (machine-readable)**: `exiftool -j input.jpg`
- **Strip all metadata**: `exiftool -all= -o cleaned.jpg input.jpg`
- **Copy metadata between files**: `exiftool -tagsFromFile source.jpg target.jpg`
- **Set copyright/artist**: `exiftool -Copyright="2026 Studio" -Artist="Name" -o tagged.jpg input.jpg`
- **Shift dates (e.g. timezone fix)**: `exiftool "-AllDates+=1:0:0 0" -o fixed.jpg input.jpg`
- **Rename by capture date**: `exiftool "-FileName<CreateDate" -d "%Y%m%d_%H%M%S.%%e" *.jpg`
- **Read RAW (CR3, ARW, NEF, DNG)**: `exiftool -ColorTemperature -ISO -ShutterSpeed input.cr3`
- **Video timecode**: `exiftool -TimeCode -StartTimecode input.mov`

### Tips
- Always use `-o output` or `-tagsFromFile` rather than in-place edits unless the user explicitly asks; ExifTool creates `*_original` backups by default but they should still be controlled.
- Use `-G` to show tag groups (`EXIF`, `XMP`, `IPTC`) when diagnosing.
- For batch ops, ExifTool itself accepts a directory: `exiftool -CreateDate dir/` is faster than a shell loop.
- ExifTool reads EXR (`OpenEXR:*` tags) and most VFX/color metadata that oiiotool exposes plus more.
