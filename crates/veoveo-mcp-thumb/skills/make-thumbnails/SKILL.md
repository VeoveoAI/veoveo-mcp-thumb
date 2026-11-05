---
name: make-thumbnails
description: Make a thumbnail of a file you can read in a Lore repository or Artifact, as a small picture. Use when a program needs a picture of a file; to look at one yourself, it is usually cheaper to read the file.
---

# Make thumbnails

Call `thumb__thumbnail` with a `source` and a `size`. The answer holds `mediaType` and `data`, the picture in base64, or neither
when no thumbnail could be made. Every read is yours: a file you may not read is refused.

- A file in Lore: `{"source": {"from": "lore", "repository": "...", "branch": "main", "path": "docs/report.docx"}}`.
- An artifact: `{"source": {"from": "artifact", "artifact": "..."}}`.
- `size` is `small`, at most 480 pixels on a side, or `large`, at most 1280. `small` is the default.

The base64 is long: it is meant for programs, and it costs a model a lot of text to read.
