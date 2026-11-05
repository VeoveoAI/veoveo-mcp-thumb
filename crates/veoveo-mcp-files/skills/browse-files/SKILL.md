---
name: browse-files
description: See what the Lore repositories you can read hold, as a list or in a gallery with a picture of each file, and give a person a link that opens one. Use when someone asks what is in a repository or wants to look through its files.
---

# Browse files

Every read is yours: a repository you hold no grant on is refused.

1. Call `files__list_files` with nothing to list the repositories you can read, each with its `id` and `name`, then with that
   `id` as `repository`, and a `branch` if it is not `main`, to list its top. Give a folder's path as `path` to list it. Each
   file says its `size` and its `kind`: `image`, `video`, `audio`, `document`, `model`, `scene` or `other`.
2. To show a person, name the App: `/apps/files/browser` on the Hub. It is a gallery with a picture of each file.
3. To open one file for a person, give the Hub's link: `/view/lore/{repository}/{branch}/{path}`, with each path segment
   percent-encoded. It opens with the right viewer, for whoever follows it and may read the file.
