# veoveo-mcp-thumb

An MCP server for Veoveo that makes thumbnails: give it a file a person can read, in Lore or Artifact, and it gives back a
small picture. It reads as the person who asked, through the Hub, so the file's owner decides every read.

It runs as a factory server and draws files with standard programs, each in a narrower sandbox of its own:

| Files | Drawn by |
|---|---|
| Word, Excel, PowerPoint, OpenDocument, HTML, text, Markdown, code | LibreOffice, then Poppler |
| PDF | Poppler |
| Images, PSD | ImageMagick; SVG with librsvg |
| Video, audio | FFmpeg, reading only the two ends of a large file |
| glTF binary, FBX, STEP, IGES, OBJ, STL and more | [F3D](https://github.com/f3d-app/f3d), on the GPU |
| USD | The [USD server](https://github.com/VeoveoAI/veoveo-mcp-usd), on the GPU |

The command lines follow [Thumbelina](https://hub.docker.com/r/sciactive/thumbelina) (Apache-2.0), used as a recipe rather
than a service because each step here needs its own limits. See
[crates/veoveo-mcp-thumb/DESIGN.md](crates/veoveo-mcp-thumb/DESIGN.md).

## Development environment

```sh
docker build --target dev -t <repository>:dev .
docker build --target check .
```

## License

Apache-2.0. The programs in `openshell/image/` keep their own licenses.
