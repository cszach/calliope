"""Cuts the demo video from scripts/demo.py's raw recording.

events.json lists the shots: where each starts in the recording, its caption,
the area to zoom in on, how fast to play it and a key combination to show.
Each output frame is cropped from the matching recorded frame, so the camera
glides between shots instead of cutting, and the captions are drawn on top.
The video ends on a card with the install command.
"""

import json
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = Path(__file__).resolve().parent.parent
ICON = REPO / "data/icons/hicolor/scalable/apps/io.github.cszach.Calliope.svg"
SIZE = (1920, 1200)
FPS = 30
SCALE = 2  # the recording's monitor scale: events are in logical pixels
# How long the camera takes to move to the next shot, in seconds of output.
GLIDE = 0.7
# The closest the camera gets: this fraction of the screen's width.
MIN_VIEW = 0.45
KEYS_SHOWN = 1.6
END_CARD = 3.5
FADE = 0.4
INSTALL = "flatpak install --user https://zachnguyen.com/calliope/calliope.flatpakref"


def font(size, weight=400, family="Adwaita Sans"):
    """GNOME's interface font by default, or whatever fontconfig offers."""
    path = subprocess.run(["fc-match", "-f", "%{file}", f"{family}:weight={weight}"],
                          capture_output=True, text=True).stdout
    return ImageFont.truetype(path, size) if path else ImageFont.load_default(size)


def ease(f):
    f = min(max(f, 0.0), 1.0)
    return f * f * (3 - 2 * f)


def view_for(focus, frame):
    """The part of the frame to show for a focus rectangle (logical pixels),
    padded, at the video's aspect ratio, and inside the frame."""
    fw, fh = frame
    if not focus:
        return (0.0, 0.0, float(fw), float(fh))
    x, y = focus["x"] * SCALE, focus["y"] * SCALE
    w, h = focus["width"] * SCALE, focus["height"] * SCALE
    pad = 0.12 * max(w, h) + 48
    w, h = w + 2 * pad, h + 2 * pad
    aspect = SIZE[0] / SIZE[1]
    if w / h < aspect:
        w = h * aspect
    else:
        h = w / aspect
    w = min(max(w, fw * MIN_VIEW), fw)
    h = w / aspect
    cx = x + focus["width"] * SCALE / 2
    cy = y + focus["height"] * SCALE / 2
    left = min(max(cx - w / 2, 0), fw - w)
    top = min(max(cy - h / 2, 0), fh - h)
    return (left, top, left + w, top + h)


def plan(events, frame, duration):
    """One entry per output frame: (source time, view box, caption, its
    opacity, keys or None, their opacity)."""
    frames = []
    previous_view = view_for(None, frame)
    previous_caption = None
    for i, shot in enumerate(events):
        if shot["caption"] is None and i == len(events) - 1:
            break
        start = shot["t"]
        end = events[i + 1]["t"] if i + 1 < len(events) else duration
        if shot.get("skip"):
            continue
        speed = shot.get("speed") or 1.0
        count = int((end - start) / speed * FPS)
        target = view_for(shot["focus"], frame)
        origin = frames[-1][1] if frames else previous_view
        caption_start = len(frames) if shot["caption"] != previous_caption else None
        for k in range(count):
            out_t = k / FPS
            f = ease(out_t / GLIDE)
            view = tuple(a + (b - a) * f for a, b in zip(origin, target))
            caption_alpha = 1.0
            if caption_start is not None:
                caption_alpha = ease(out_t / 0.3)
            keys_alpha = 0.0
            if shot.get("keys") and out_t < KEYS_SHOWN:
                keys_alpha = ease(out_t / 0.15) * (1 - ease((out_t - KEYS_SHOWN + 0.3) / 0.3))
            frames.append((start + out_t * speed, view, shot["caption"], caption_alpha,
                           shot.get("keys"), keys_alpha))
        previous_view = target
        previous_caption = shot["caption"]
    return frames


def pill(text, typeface, padding=(36, 18), fill=(30, 30, 30, 215), color=(255, 255, 255, 255)):
    """Text on a rounded, translucent dark background, like a GNOME toast."""
    left, top, right, bottom = typeface.getbbox(text)
    w, h = right - left + 2 * padding[0], bottom - top + 2 * padding[1]
    image = Image.new("RGBA", (w, h))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((0, 0, w - 1, h - 1), radius=h // 2, fill=fill)
    draw.text((padding[0] - left, padding[1] - top), text, font=typeface, fill=color)
    return image


def keycaps(keys, typeface):
    """Ctrl+Alt+M as three key caps."""
    caps = [pill(k, typeface, padding=(26, 16), fill=(250, 250, 250, 240),
                 color=(30, 30, 30, 255)) for k in keys.split("+")]
    gap = 14
    w = sum(c.width for c in caps) + gap * (len(caps) - 1)
    h = max(c.height for c in caps)
    image = Image.new("RGBA", (w + 24, h + 24))
    x = 12
    shadow = Image.new("RGBA", image.size)
    for cap in caps:
        shadow.paste((0, 0, 0, 90), (x, 18, x + cap.width, 18 + cap.height), cap)
        x += cap.width + gap
    image = Image.alpha_composite(image, shadow.filter(ImageFilter.GaussianBlur(6)))
    x = 12
    for cap in caps:
        image.alpha_composite(cap, (x, 12))
        x += cap.width + gap
    return image


def with_alpha(image, alpha):
    if alpha >= 1:
        return image
    faded = image.copy()
    faded.putalpha(faded.getchannel("A").point(lambda a: int(a * alpha)))
    return faded


def icon(size):
    """Calliope's icon, rendered through GdkPixbuf's SVG loader."""
    try:
        import gi
        gi.require_version("GdkPixbuf", "2.0")
        from gi.repository import GdkPixbuf
        pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_size(str(ICON), size, size)
        ok, data = pixbuf.save_to_bufferv("png", [], [])
        import io
        return Image.open(io.BytesIO(data)).convert("RGBA")
    except Exception:  # an end card without the icon is still an end card
        return None


def end_card():
    card = Image.new("RGBA", SIZE, (250, 250, 250, 255))
    draw = ImageDraw.Draw(card)
    title, body, small = font(96, 800), font(44), font(30)
    mono = font(30, family="Adwaita Mono")
    y = 250
    art = icon(256)
    if art:
        card.alpha_composite(art, ((SIZE[0] - art.width) // 2, y))
        y += art.height + 40
    for text, face, color, gap in (
            ("Calliope", title, (30, 30, 30), 30),
            ("A GNOME client for Muse", body, (80, 80, 80), 70),
            (INSTALL, mono, (30, 30, 30), 40),
            ("Unofficial. Not affiliated with or endorsed by Meta.", small, (110, 110, 110), 0)):
        left, top, right, bottom = draw.textbbox((0, 0), text, font=face)
        draw.text(((SIZE[0] - (right - left)) // 2 - left, y - top), text, font=face, fill=color)
        y += bottom - top + gap
    return card.convert("RGB")


def edit(out):
    raw, events = out / "raw.mkv", json.loads((out / "events.json").read_text())
    probe = json.loads(subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
         "stream=width,height:format=duration", "-of", "json", str(raw)],
        capture_output=True, text=True, check=True).stdout)
    frame = (probe["streams"][0]["width"], probe["streams"][0]["height"])
    duration = float(probe["format"]["duration"])
    frames = plan(events, frame, duration)
    total = len(frames) / FPS + END_CARD
    print(f"Video: {total:.1f} s from {duration:.1f} s recorded, {len(events)} shots")
    if total > 60:
        print("warning: the video is longer than a minute")

    caption_face, keys_face = font(46, 700), font(40, 700)
    captions, caps = {}, {}
    decoder = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-i", str(raw), "-f", "rawvideo", "-pix_fmt", "rgb24",
         "-r", str(FPS), "-"], stdout=subprocess.PIPE)
    encoder = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
         "-s", f"{SIZE[0]}x{SIZE[1]}", "-r", str(FPS), "-i", "-",
         "-c:v", "libx264", "-preset", "slow", "-crf", "24", "-pix_fmt", "yuv420p",
         "-movflags", "+faststart", str(out / "demo.mp4")], stdin=subprocess.PIPE)
    frame_bytes = frame[0] * frame[1] * 3
    source_index, source = -1, None
    last = None
    for t, view, caption, caption_alpha, keys, keys_alpha in frames:
        wanted = round(t * FPS)
        while source_index < wanted:
            data = decoder.stdout.read(frame_bytes)
            if len(data) < frame_bytes:
                break
            source, source_index = data, source_index + 1
        if source is None:
            raise RuntimeError(f"{raw} has no frames")
        image = Image.frombuffer("RGB", frame, source).resize(SIZE, Image.Resampling.BICUBIC,
                                                              box=view).convert("RGBA")
        if caption:
            if caption not in captions:
                captions[caption] = pill(caption, caption_face)
            art = with_alpha(captions[caption], caption_alpha)
            image.alpha_composite(art, ((SIZE[0] - art.width) // 2, SIZE[1] - art.height - 64))
        if keys and keys_alpha > 0:
            if keys not in caps:
                caps[keys] = keycaps(keys, keys_face)
            art = with_alpha(caps[keys], keys_alpha)
            image.alpha_composite(art, ((SIZE[0] - art.width) // 2, SIZE[1] - art.height - 170))
        last = image.convert("RGB")
        encoder.stdin.write(last.tobytes())
    decoder.kill()
    decoder.wait()
    card = end_card()
    for k in range(int(END_CARD * FPS)):
        f = ease(k / (FADE * FPS))
        encoder.stdin.write(Image.blend(last, card, f).tobytes() if last else card.tobytes())
    encoder.stdin.close()
    status = encoder.wait()
    size = (out / "demo.mp4").stat().st_size / 1e6
    print(f"Wrote {out / 'demo.mp4'} ({size:.1f} MB)")
    return status
