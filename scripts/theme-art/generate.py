#!/usr/bin/env python3
# SPDX-License-Identifier: CPAL-1.0
# Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
"""Generate BoothSoft theme artwork (logo, welcome background, print frame).

Everything is drawn at 2x and downsampled with Lanczos for clean anti-aliasing.
Outputs go to <themes_dir>/<id>/assets/. Run: gen_assets.py THEMES_DIR
"""
import math, os, random, sys
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
F = lambda name: os.path.join(HERE, 'fonts', name)
SS = 2  # supersampling factor

FRAME_W, FRAME_H = 1800, 1200      # 6x4 in at 300 DPI (DNP / dye-sub standard)
BG_W, BG_H = 1600, 1000            # welcome artwork, drawn "cover" by the kiosk
LOGO_W, LOGO_H = 720, 200


def hexc(value, alpha=255):
    value = value.lstrip('#')
    return tuple(int(value[i:i + 2], 16) for i in (0, 2, 4)) + (alpha,)


def font(name, size):
    return ImageFont.truetype(F(name), size)


def canvas(w, h, color=(0, 0, 0, 0)):
    return Image.new('RGBA', (w * SS, h * SS), color)


def finish(img, w, h):
    return img.resize((w, h), Image.LANCZOS)


def vgradient(w, h, top, bottom):
    t = np.linspace(0, 1, h)[:, None, None]
    a, b = np.array(hexc(top), float), np.array(hexc(bottom), float)
    arr = (a + (b - a) * t) * np.ones((h, w, 1))
    return Image.fromarray(arr.astype(np.uint8), 'RGBA')


def radial_glow(w, h, cx, cy, radius, color, strength=1.0):
    y, x = np.ogrid[:h, :w]
    d = np.sqrt((x - cx) ** 2 + (y - cy) ** 2) / radius
    alpha = np.clip(1 - d, 0, 1) ** 2 * 255 * strength
    arr = np.zeros((h, w, 4), np.uint8)
    arr[..., :3] = hexc(color)[:3]
    arr[..., 3] = alpha.astype(np.uint8)
    return Image.fromarray(arr, 'RGBA')


def grain(img, amount=6, seed=1):
    rng = np.random.default_rng(seed)
    arr = np.asarray(img).astype(np.int16)
    noise = rng.normal(0, amount, arr.shape[:2])[..., None]
    arr[..., :3] = np.clip(arr[..., :3] + noise, 0, 255)
    return Image.fromarray(arr.astype(np.uint8), 'RGBA')


def string_lights(img, p0, p1, sag, count, wire, bulb, glow, r):
    d = ImageDraw.Draw(img)
    pts = []
    for i in range(61):
        t = i / 60
        x = p0[0] + (p1[0] - p0[0]) * t
        y = p0[1] + (p1[1] - p0[1]) * t + sag * 4 * t * (1 - t)
        pts.append((x, y))
    d.line(pts, fill=hexc(wire), width=max(2, r // 4), joint='curve')
    glow_layer = Image.new('RGBA', img.size, (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow_layer)
    bulbs = []
    for i in range(1, count + 1):
        t = i / (count + 1)
        x = p0[0] + (p1[0] - p0[0]) * t
        y = p0[1] + (p1[1] - p0[1]) * t + sag * 4 * t * (1 - t) + r * 1.4
        bulbs.append((x, y))
        gd.ellipse((x - r * 3.2, y - r * 3.2, x + r * 3.2, y + r * 3.2), fill=hexc(glow, 70))
    img.alpha_composite(glow_layer.filter(ImageFilter.GaussianBlur(r * 2)))
    d = ImageDraw.Draw(img)
    for x, y in bulbs:
        d.line([(x, y - r * 1.4), (x, y - r * 0.6)], fill=hexc(wire), width=max(2, r // 4))
        d.ellipse((x - r * 0.8, y - r, x + r * 0.8, y + r * 1.2), fill=hexc(bulb))
        d.ellipse((x - r * 0.35, y - r * 0.5, x + r * 0.05, y), fill=(255, 255, 255, 170))


def confetti(img, box, count, colors, size, seed, shapes=('rect', 'dot', 'stripe')):
    rng = random.Random(seed)
    for _ in range(count):
        x = rng.uniform(box[0], box[2]); y = rng.uniform(box[1], box[3])
        c = hexc(rng.choice(colors), rng.randint(170, 255))
        s = size * rng.uniform(0.6, 1.3)
        shape = rng.choice(shapes)
        piece = Image.new('RGBA', (int(s * 4), int(s * 4)), (0, 0, 0, 0))
        pd = ImageDraw.Draw(piece)
        m = s * 2
        if shape == 'dot':
            pd.ellipse((m - s * 0.5, m - s * 0.5, m + s * 0.5, m + s * 0.5), fill=c)
        elif shape == 'rect':
            pd.rounded_rectangle((m - s * 0.45, m - s, m + s * 0.45, m + s), radius=s * 0.15, fill=c)
        else:
            pd.arc((m - s * 1.4, m - s * 0.7, m + s * 1.4, m + s * 1.6), 200, 340, fill=c, width=max(2, int(s * 0.35)))
        piece = piece.rotate(rng.uniform(0, 360), resample=Image.BICUBIC)
        img.alpha_composite(piece, (int(x - m), int(y - m)))


def balloon(img, cx, cy, r, color, string_color, seed):
    rng = random.Random(seed)
    shadow = Image.new('RGBA', img.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).ellipse((cx - r + r * .15, cy - r * 1.15 + r * .2, cx + r + r * .15, cy + r * 1.15 + r * .2), fill=(0, 0, 0, 70))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(r * 0.25)))
    d = ImageDraw.Draw(img)
    pts = []
    sway = rng.uniform(-1, 1)
    for i in range(30):
        t = i / 29
        pts.append((cx + math.sin(t * 5 + sway) * r * 0.15, cy + r * 1.2 + t * r * 3.2))
    d.line(pts, fill=hexc(string_color, 200), width=max(2, int(r * 0.035)), joint='curve')
    d.polygon([(cx - r * .12, cy + r * 1.22), (cx + r * .12, cy + r * 1.22), (cx, cy + r * 1.08)], fill=hexc(color))
    body = Image.new('RGBA', img.size, (0, 0, 0, 0))
    bd = ImageDraw.Draw(body)
    bd.ellipse((cx - r, cy - r * 1.15, cx + r, cy + r * 1.15), fill=hexc(color))
    shade = radial_glow(img.size[0], img.size[1], cx + r * 0.45, cy + r * 0.55, r * 1.3, '#000000', 0.35)
    hl = radial_glow(img.size[0], img.size[1], cx - r * 0.35, cy - r * 0.45, r * 0.75, '#FFFFFF', 0.55)
    mask = body.split()[3]
    body.alpha_composite(Image.composite(shade, Image.new('RGBA', img.size, (0, 0, 0, 0)), mask))
    body.alpha_composite(Image.composite(hl, Image.new('RGBA', img.size, (0, 0, 0, 0)), mask))
    img.alpha_composite(body)


# --------------------------------------------------------------------------- themes


def party(out, band_text="QUE LA FÊTE COMMENCE !"):
    plum, night, pink, yellow, teal, orange, cream = '#2A1B47', '#1A1030', '#F0477D', '#FFC857', '#3DD6C4', '#FF8A4C', '#FFF6EC'
    W, H = BG_W * SS, BG_H * SS
    bg = vgradient(W, H, '#2D1D4D', night)
    for cx, cy, r, c, s in [(0.80, 0.40, 0.65, '#6B2E7A', 0.75), (0.15, 0.95, 0.5, '#3B2470', 0.6), (0.62, 0.05, 0.4, '#5A2A6E', 0.5)]:
        bg.alpha_composite(radial_glow(W, H, W * cx, H * cy, H * r, c, s))
    # light rays
    rays = Image.new('RGBA', bg.size, (0, 0, 0, 0))
    rd = ImageDraw.Draw(rays)
    for k in range(9):
        a = -0.2 + k * 0.17
        rd.polygon([(W * 0.80, H * 1.05), (W * 0.80 + math.cos(-a - 1.2) * W, H * 1.05 + math.sin(-a - 1.2) * W),
                    (W * 0.80 + math.cos(-a - 1.12) * W, H * 1.05 + math.sin(-a - 1.12) * W)], fill=(255, 255, 255, 10))
    bg.alpha_composite(rays.filter(ImageFilter.GaussianBlur(20)))
    # streamers garland
    string_lights(bg, (W * 0.40, -H * 0.03), (W * 1.03, H * 0.16), H * 0.13, 10, '#8D7BB5', yellow, '#FFB347', int(H * 0.011))
    confetti(bg, (W * 0.50, H * 0.05, W, H), 200, [pink, yellow, teal, orange, '#B18CFF'], H * 0.014, 5)
    confetti(bg, (0, H * 0.86, W * 0.45, H), 14, [pink, yellow, teal], H * 0.010, 6)
    for cx, cy, r, c, s in [(0.70, 0.46, 0.085, pink, 1), (0.83, 0.33, 0.095, yellow, 2), (0.94, 0.52, 0.08, teal, 3), (0.77, 0.66, 0.07, orange, 4), (0.90, 0.80, 0.06, '#B18CFF', 5)]:
        balloon(bg, W * cx, H * cy, H * r, c, '#D9CCF2', s)
    bg = grain(finish(bg, BG_W, BG_H), 3, 9)
    bg.convert('RGB').save(os.path.join(out, 'background.png'), optimize=True)

    lg = canvas(LOGO_W, LOGO_H)
    d = ImageDraw.Draw(lg)
    pf = font('Poppins-Bold.ttf', 124 * SS)
    x = 60 * SS
    for ch, col in zip("PARTY!", [pink, yellow, teal, orange, '#B18CFF', pink]):
        d.text((x, 104 * SS), ch, font=pf, fill=hexc(col), anchor='lm')
        x += d.textlength(ch, font=pf) + 8 * SS
    finish(lg, LOGO_W, LOGO_H).save(os.path.join(out, 'logo.png'), optimize=True)

    W, H = FRAME_W * SS, FRAME_H * SS
    fr = canvas(FRAME_W, FRAME_H)
    d = ImageDraw.Draw(fr)
    m, band = 44 * SS, 200 * SS
    d.rectangle((0, 0, W, H), fill=hexc(plum))
    border = Image.new('RGBA', fr.size, (0, 0, 0, 0))
    cols = [pink, yellow, teal, orange, '#B18CFF']
    for k, box in enumerate([(0, 0, W, m), (0, 0, m, H), (W - m, 0, W, H), (0, H - band, W * 0.17, H), (W * 0.83, H - band, W, H), (0, H - 24 * SS, W, H)]):
        area = (box[2] - box[0]) * (box[3] - box[1]) / (W * H)
        confetti(border, box, int(1400 * area) + 8, cols, 15 * SS, 41 + k)
    fr.alpha_composite(border)
    d = ImageDraw.Draw(fr)
    d.rounded_rectangle((m, m, W - m, H - band), radius=26 * SS, fill=(0, 0, 0, 0))
    d.rounded_rectangle((m - 8 * SS, m - 8 * SS, W - m + 8 * SS, H - band + 8 * SS), radius=32 * SS, outline=hexc(yellow), width=5 * SS)
    d.text((W / 2, H - band / 2), band_text, font=font('Poppins-Bold.ttf', 84 * SS), fill=hexc(cream), anchor='mm')
    finish(fr, FRAME_W, FRAME_H).save(os.path.join(out, 'frame.png'), optimize=True)


def default(out):
    """The bundled theme, shown as "Party": generic and festive, no brand, no occasion."""
    party(out, band_text="QUE LA FÊTE COMMENCE !")
    party_portrait(out, ["QUE LA FÊTE", "COMMENCE !"])


# Portrait print (4 x 6 in): same paper turned. The photo window matches the
# landscape layout turned on its side; BoothSoft finds it from the transparency.
PORTRAIT_W, PORTRAIT_H = 1200, 1800
PORTRAIT_WINDOW = (96, 300, 1008, 1200)   # x, y, width, height


def party_portrait(out, band_lines):
    plum, pink, yellow, teal, orange, cream = '#2A1B47', '#F0477D', '#FFC857', '#3DD6C4', '#FF8A4C', '#FFF6EC'
    W, H = PORTRAIT_W * SS, PORTRAIT_H * SS
    x, y, w, h = (v * SS for v in PORTRAIT_WINDOW)
    fr = canvas(PORTRAIT_W, PORTRAIT_H)
    d = ImageDraw.Draw(fr)
    d.rectangle((0, 0, W, H), fill=hexc(plum))
    border = Image.new('RGBA', fr.size, (0, 0, 0, 0))
    cols = [pink, yellow, teal, orange, '#B18CFF']
    for k, box in enumerate([(0, 0, W, y), (0, 0, x, H), (W - x, 0, W, H), (0, y + h, W, H)]):
        area = (box[2] - box[0]) * (box[3] - box[1]) / (W * H)
        confetti(border, box, int(1400 * area) + 8, cols, 15 * SS, 71 + k)
    fr.alpha_composite(border)
    d = ImageDraw.Draw(fr)
    # Title on top: the PARTY! letters, then the photo, then the band text.
    pf = font('Poppins-Bold.ttf', 150 * SS)
    letters = "PARTY!"
    total = sum(d.textlength(ch, font=pf) for ch in letters) + 8 * SS * (len(letters) - 1)
    cx = (W - total) / 2
    for ch, col in zip(letters, [pink, yellow, teal, orange, '#B18CFF', pink]):
        d.text((cx, y / 2 + 6 * SS), ch, font=pf, fill=hexc(col), anchor='lm')
        cx += d.textlength(ch, font=pf) + 8 * SS
    d.rounded_rectangle((x, y, x + w, y + h), radius=26 * SS, fill=(0, 0, 0, 0))
    d.rounded_rectangle((x - 8 * SS, y - 8 * SS, x + w + 8 * SS, y + h + 8 * SS), radius=32 * SS, outline=hexc(yellow), width=5 * SS)
    bf = font('Poppins-Bold.ttf', 76 * SS)
    band_top, band_h = y + h, H - (y + h)
    for i, line in enumerate(band_lines):
        d.text((W / 2, band_top + band_h * (i + 1) / (len(band_lines) + 1)), line, font=bf, fill=hexc(cream), anchor='mm')
    finish(fr, PORTRAIT_W, PORTRAIT_H).save(os.path.join(out, 'frame-portrait.png'), optimize=True)


def trim_logo(path, pad=8):
    img = Image.open(path)
    box = img.getbbox()
    if box:
        box = (max(0, box[0] - pad), max(0, box[1] - pad), min(img.width, box[2] + pad), min(img.height, box[3] + pad))
        img.crop(box).save(path, optimize=True)


if __name__ == '__main__':
    root = sys.argv[1]
    for theme_id, fn in [('default', default)]:
        out = os.path.join(root, theme_id, 'assets')
        os.makedirs(out, exist_ok=True)
        fn(out)
        trim_logo(os.path.join(out, 'logo.png'))
        print('generated', theme_id)
