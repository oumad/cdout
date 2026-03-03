---
name: ffmpeg
description: Video and audio processing with FFmpeg
requires:
  bins: [ffmpeg]
---
You have **FFmpeg** available for video/audio processing. Use it for:

### Common Operations
- **Convert**: `ffmpeg -i input.mp4 output.webm`
- **Trim**: `ffmpeg -i input.mp4 -ss 00:00:10 -to 00:00:30 -c copy output.mp4`
- **Extract audio**: `ffmpeg -i input.mp4 -vn -acodec libmp3lame output.mp3`
- **Resize**: `ffmpeg -i input.mp4 -vf "scale=1280:720" output.mp4`
- **Concatenate**: Use a file list with `ffmpeg -f concat -safe 0 -i list.txt -c copy output.mp4`
- **GIF**: `ffmpeg -i input.mp4 -vf "fps=15,scale=480:-1:flags=lanczos" output.gif`
- **Compress**: `ffmpeg -i input.mp4 -crf 28 -preset medium output.mp4`
- **Strip audio**: `ffmpeg -i input.mp4 -an output.mp4`
- **Change FPS**: `ffmpeg -i input.mp4 -r 30 output.mp4`
- **Reverse**: `ffmpeg -i input.mp4 -vf reverse -af areverse output.mp4`

### Tips
- Use `-c copy` for lossless stream copy (no re-encoding) when only cutting/merging.
- Use `-crf` (18-28) for quality control with H.264/H.265.
- For batch ops, use PowerShell `foreach` loops over `$files`.
- Always output to new files, never overwrite the input.
