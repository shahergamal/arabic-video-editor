ffmpeg and ffprobe go here, named with the target triple, for example
`ffmpeg-x86_64-pc-windows-msvc.exe` or `ffmpeg-aarch64-apple-darwin`.
The release workflow (`.github/workflows/release.yml`) downloads them; they are
not committed. For local development: `node ../../scripts/fetch-ffmpeg.mjs`.
