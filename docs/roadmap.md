# Roadmap

Full plan (Arabic): https://claude.ai/artifact/JwQU4TJLXP4Um8ZYtKpH5M

Status: step 0 ✓ (PR #1), phase 1 ✓ (PR #2), phase 2 ✓ (PR #3), phase 3 ✓ (PR #4), phase 4 ✓ (PR #5).

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

## Phase 4 notes

- **HDR:** HDR10/PQ and HLG files are detected from their colour tags. They
  are tone-mapped to normal (BT.709) colour with zimg + a Hable curve, in the
  preview, the export and the thumbnails. They also get a normal-colour proxy,
  because tone-mapping every frame is slow. Export is SDR, which is what social
  platforms show to most viewers. HDR export would need a 10-bit pipeline,
  and is left for later.
- **Multi-camera:** cameras are lined up by their sound. The engine compares
  loudness onsets with an FFT cross-correlation at 100/s, then refines at
  1000/s, so the result is within about 1 ms (tested). A low-confidence match
  is flagged. The cut is one muted video track (`mc`) whose clips switch
  camera; the sound is one continuous clip from the first camera. Press 1–9,
  or the camera buttons, to cut at the playhead, live while playing too.
- **Other editors:**
  - OpenTimelineIO can be read and written, including gaps, speed and volume.
  - Final Cut Pro 7 XML can be read and written; this is the format Premiere
    Pro and DaVinci Resolve import and export.
  - EDL (CMX 3600) can be written; it holds one picture track and one audio
    track.
  - Texts, effects, motion and transitions do not move across, and the app
    says which of these were left out.
- **Plugins:** data only (looks, LUTs, text styles, fonts); see
  [plugins.md](plugins.md).

## Phase 5 notes

- **Picture tools:** rotation (keyframed), mirror, crop, and blend modes
  (multiply, screen, overlay, add, darken, lighten). Order per clip: colour
  (wheels → curves → LUT) → green screen → cut-out → mask → crop → mirror →
  position/size/rotation → blend.
- **Time tools:** freeze frame (F), play backwards, and steadying (ffmpeg
  vidstab, falling back to deshake). Backwards and steadied clips play from a
  working copy kept in the app's data folder; the original file is never
  touched. Backwards clips are silent in the live preview but have sound in
  the export.
- **Colour:** curves (master, red, green, blue; monotone cubic so they never
  wiggle back) and three colour wheels (shadows, midtones, highlights).
- **Shapes and stickers:** rectangle, circle, line, arrow and star, drawn with
  smooth edges, with the same animations as texts plus "draw" for lines and
  arrows. Stickers are ready-made shape recipes.
- **Groups:** several clips become one group clip (Ctrl+G) that moves, trims
  and fades as one, and can be taken apart again (Ctrl+Shift+G). Groups are
  flattened when exchanging timelines with Premiere/Resolve.
- **Editing:** copy, paste, duplicate, many clips at once (Ctrl/Shift+click),
  detach audio, close gaps, slip (Alt+drag the middle), roll (Alt+drag an
  edge), J/K/L playback, markers (M), and ↑/↓ to jump between cuts and
  markers.
- **Sound:** voice-over recording from the microphone, saved as WAV in the
  app's data folder. Captions can be translated to English while listening
  (Whisper translate).
- **Beats:** onset strength → tempo by autocorrelation → beat tracking by
  dynamic programming. "Cut to the beat" retimes a track's clips so every cut
  lands on a beat and no clip runs past its file.
- **Follow the subject:** for another shape (e.g. 16:9 → 9:16), the person is
  found with the cut-out model four times a second, the path is smoothed and
  simplified, and becomes position keyframes.
- **Graphics-card export:** NVIDIA, Intel, AMD and Apple encoders are tried
  once with a tiny test; if the real export fails it reruns on the processor.
- **Automatic edit:** each shot is scored for sharpness, light, motion and
  shake; the idea is turned into a plan (shots, order, length, texts, music)
  by built-in rules (free), a small local model through llama.cpp (free,
  offline after one download), or Claude Haiku with the user's key (shows the
  cost first, keeps to a monthly limit, sends text only). The plan is checked
  and repaired before it becomes a normal, editable project.
- **Publishing:** Facebook (post, reel, story), Instagram (post, reel,
  story) and TikTok, through the user's own developer apps, uploaded only
  when Publish is pressed. Without connected accounts the app exports the
  right shapes, copies the caption and opens the sites. See
  [publishing-ar.md](publishing-ar.md).
- **Updates:** the home screen says when a newer release is out.
