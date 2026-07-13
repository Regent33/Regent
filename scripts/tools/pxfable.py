"""
pxfable — render a long prompt as PNG page(s) for pxpipe-style image-token savings.

Image tokens are priced by pixel area (w*h/750), not by how much text is inside.
This just converts text -> PNG files; attach them to Claude yourself.

Usage:
  python scripts/pxfable.py big_file.txt              # writes big_file.p1.png, ...
  type big_file.txt | python scripts/pxfable.py       # writes prompt.p1.png, ...
  python scripts/pxfable.py --check                   # offline self-test

WARNING (pxpipe's own benchmarks): the image path is LOSSY for verbatim strings —
hashes, names, IDs can be confabulated silently. Gist/reasoning is fine.
"""
import argparse
import io
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# ponytail: single column, safe font size. pxpipe packs ~92k chars/page with
# tighter type; shrink FONT_SIZE toward 10 for more savings at more OCR risk.
FONT_SIZE = 13
PAGE_W = 1928
PAGE_H = 1945  # w*h ~= 3.75MP, Fable's no-downscale ceiling
TOKENS_PER_PAGE = PAGE_W * PAGE_H // 750  # ~5000


def _font():
    try:
        return ImageFont.truetype("consola.ttf", FONT_SIZE)
    except OSError:
        return ImageFont.load_default(FONT_SIZE)


def render_pages(text: str) -> list[bytes]:
    """Rasterize text into PNG pages, black-on-white monospace."""
    font = _font()
    char_w = font.getbbox("M")[2] or FONT_SIZE
    line_h = FONT_SIZE + 3
    cols = PAGE_W // char_w
    rows = PAGE_H // line_h

    lines = []
    for raw in text.splitlines() or [""]:
        while len(raw) > cols:
            lines.append(raw[:cols])
            raw = raw[cols:]
        lines.append(raw)

    pages = []
    for i in range(0, len(lines), rows):
        chunk = lines[i:i + rows]
        # crop width to actual content — blank pixels cost tokens too
        w = min(PAGE_W, max(len(l) for l in chunk) * char_w + 4)
        img = Image.new("L", (w, min(PAGE_H, len(chunk) * line_h + 4)), 255)
        draw = ImageDraw.Draw(img)
        for row, line in enumerate(chunk):
            draw.text((2, row * line_h), line, font=font, fill=0)
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        pages.append(buf.getvalue())
    return pages


def image_token_estimate(pages: list[bytes]) -> int:
    total = 0
    for png in pages:
        w, h = Image.open(io.BytesIO(png)).size
        total += w * h // 750
    return total


def convert(text: str, stem: str) -> None:
    pages = render_pages(text)
    for n, png in enumerate(pages, 1):
        Path(f"{stem}.p{n}.png").write_bytes(png)
    img_tokens = image_token_estimate(pages)
    text_tokens = len(text) // 4  # rough English estimate; code/JSON is worse (more tokens)
    print(f"{len(pages)} page(s) -> {stem}.p1.png ..")
    print(f"~{img_tokens} image tokens vs ~{text_tokens}+ as text "
          f"({len(text) / img_tokens:.1f} chars/img-token)")


def check():
    """Offline self-test: rendering, pagination, and the token math."""
    text = "\n".join(f"line {i}: " + "x" * 300 for i in range(400))
    pages = render_pages(text)
    assert len(pages) >= 1
    img = Image.open(io.BytesIO(pages[0]))
    assert img.size[0] == PAGE_W and img.size[1] <= PAGE_H
    est = image_token_estimate(pages)
    assert 0 < est <= len(pages) * TOKENS_PER_PAGE
    density = len(text) / est
    assert density > 3.5, f"density {density:.1f} chars/img-token — no arbitrage, shrink FONT_SIZE"
    assert render_pages("short") and image_token_estimate(render_pages("short")) < TOKENS_PER_PAGE
    print(f"ok: {len(pages)} pages, ~{est} img tokens, {density:.1f} chars/img-token")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("file", nargs="?", help="text file to convert (default: stdin)")
    ap.add_argument("--check", action="store_true", help="offline self-test")
    args = ap.parse_args()
    if args.check:
        check()
    elif args.file:
        convert(Path(args.file).read_text(encoding="utf-8-sig"), Path(args.file).stem)
    else:
        convert(sys.stdin.read(), "prompt")
