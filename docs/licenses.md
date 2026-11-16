# Licences

## Fonts
Only fonts under the SIL Open Font License or Apache 2.0 are bundled. Each lives
in `assets/fonts/<id>/` with its `LICENSE.txt`; `assets/fonts/manifest.json` lists
source URL, licence and whether commercial use is allowed. Commercial fonts are
never bundled; the app reads every font installed on the computer, so fonts the
user owns work as-is.

## FFmpeg
ffmpeg runs as a separate program (never linked into the app). The installers
currently ship **GPL** builds (BtbN for Windows/Linux, Martin Riedl for macOS),
which include the x264 encoder. This is fine for personal use.

Before selling the app:
1. Switch to LGPL builds.
2. Encode H.264/HEVC with the operating system or graphics card encoders
   (`h264_videotoolbox`, `h264_mf`, `h264_nvenc`, `h264_qsv`, `h264_amf`).
3. Have a lawyer review H.264/HEVC patent licensing for the sales countries.

## Interface
All interface design, icons and future templates/effects are original to this
project. Nothing is copied from CapCut, Premiere or any other editor.

## Downloaded on request (not bundled)

| What | Licence | Where from |
| --- | --- | --- |
| Whisper speech models (ggml) | MIT | huggingface.co/ggerganov/whisper.cpp |
| U²-Net / U²-Net-p / U²-Net human segmentation (ONNX) | Apache-2.0 | github.com/danielgatis/rembg releases |
| Qwen2.5-1.5B-Instruct, Q4_K_M GGUF (local automatic-edit planner) | Apache-2.0 | huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF |
| llama.cpp `llama-server` (runs the local model as a separate program) | MIT | github.com/ggml-org/llama.cpp releases |

## Online services the user connects themselves (nothing is ours)

| Service | Used for | When |
| --- | --- | --- |
| Anthropic Claude API (`claude-haiku-5-5`), with the user's own key | Planning the automatic edit | Only when the user picks Claude; sends text only (the idea and a description of the shots), never video or sound; stops at the user's monthly limit |
| Meta Graph API, through the user's own Meta developer app | Posting to Facebook pages and Instagram business/creator accounts | Only when the user presses Publish |
| TikTok Content Posting API, through the user's own TikTok developer app | Posting to TikTok | Only when the user presses Publish |
| GitHub releases list of this project | The "new version" notice | At most once a day; nothing about the user is sent |

Stickers and shapes are drawn by the app's own code from simple geometry;
no sticker art is bundled.
