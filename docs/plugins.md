# Plugins

A plugin is a folder with a `plugin.json` file and the files it names.
Install it from **Assistant → Plugins → Install a plugin from a folder**.
A working example is in [`plugin-example/`](plugin-example/).

Plugins only carry **data**. They never run code, so installing one cannot
harm the computer. A plugin can contain:

| Key | What it adds | Where it shows |
| --- | --- | --- |
| `looks` | Colour looks: any of `brightness`, `contrast`, `saturation`, `temperature`, `shadows`, `highlights`, `tint` (−1…1), `vignette` (0…1), plus an optional `.cube` LUT | Clip → Colour → "From plugins" |
| `texts` | Text styles: any text-layer field (`text`, `size`, `weight`, `color`, `background`, `outline`, `x`, `y`, `box_width`, `align`, `font_family`) | Texts tab → "From plugins" |
| `fonts` | `.ttf` / `.otf` files, added like the user's own fonts | Fonts tab |

Rules checked on install:

- `id` must be lowercase letters, digits and dashes. Installing the same id
  again replaces the old version.
- Every file must be inside the plugin folder. Absolute paths, `..`, and
  links are refused.
- Only these file types are allowed: `.cube`, `.ttf`, `.otf`, `.png`,
  `.jpg`, `.md` and `.txt`. Each file can be at most 64 MB.
- LUTs must parse, and look settings must be numbers.
- `name_ar` is used when the app is in Arabic.
- State the licence of every font and LUT in `license`. Only ship what you
  are allowed to share.
