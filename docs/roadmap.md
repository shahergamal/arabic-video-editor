# Roadmap

Full plan (Arabic): https://claude.ai/artifact/JwQU4TJLXP4Um8ZYtKpH5M

Status: step 0 ✓ (PR #1), phase 1 ✓ (PR #2), phase 2 ✓ (PR #3), phase 3 ✓ (PR #4).

- **Step 0 – proof:** play video, correct Arabic text over it, export that matches
  the preview. Measured preview speed shown on screen.
- **Phase 1 – foundation:** projects, import, multi-track timeline, cut / ripple,
  Arabic text + font manager, basic audio, MP4 export, autosave + crash recovery.
- **Phase 2 – production tools:** automatic Arabic captions (on-device), text
  animation, transitions, effects, keyframes, proxies, multiple sizes, templates,
  brand kits, export queue.
- **Phase 3 – pro:** colour tools + scopes, advanced audio, masks + tracking,
  background removal, speed ramps, smart assistant, real-estate templates.
- **Phase 4 – later:** multicam, HDR, timeline interchange (XML/EDL/OTIO), plugins.

## Phase 2 notes

- Captions use whisper.cpp (through `whisper-rs`) on the user's computer. The
  speech model (148 MB fast / 574 MB accurate) downloads once from Hugging Face
  after the user presses Download; sound never leaves the computer.
- Text animations work on whole words, so Arabic letters always stay joined.
- Transitions hold the outgoing clip's last frame when the file has no more
  material, so preview and export always match.
- Proxies: videos above 1080p get a 540p copy for the preview only; export
  always reads the originals.

## Phase 3 notes

- Background removal runs U²-Net (Apache-2.0, from the rembg project's
  releases) through `tract`, a pure-Rust ONNX runtime, so nothing extra has to
  be installed. Two models: "people" (176 MB) and "objects" (5 MB). They
  download once when the user presses Download; pictures never leave the
  computer. Masks are cached on disk per 1/30 s of the source, and "Prepare"
  works them out ahead of time on several threads so playback stays smooth.
- Order of picture effects for each clip: colour → LUT → green screen →
  background removal → mask. Preview and export use the same code.
- Tracking follows a box with mean-subtracted block matching plus velocity
  prediction; the result is a mask path the user can clear.
- Speed ramps keep the same part of the file when switched on or off. Clips
  with a ramp are silent (their sound would be pitch-warped).
- Audio: per-clip noise reduction (afftdn) and voice clarity (EQ + gentle
  compression); music ducking with sidechaincompress; platform loudness with
  two-pass-free `loudnorm` at −14 LUFS.
- Assistant: cut silences (ffmpeg silencedetect), a pre-publish check, and a
  quick property video built from a few facts in the brand font and colour.
