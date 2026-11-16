# Roadmap

Full plan (Arabic): https://claude.ai/artifact/JwQU4TJLXP4Um8ZYtKpH5M

Status: step 0 ✓ (PR #1), phase 1 ✓ (PR #2), phase 2 ✓ (PR #3).

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
