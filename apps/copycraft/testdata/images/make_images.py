#!/usr/bin/env python3
"""Regenerates the committed Copycraft test images (synthetic, small).

Needs python3 with Pillow and qrcode (box/dev machine only, not a crate dependency):
    python3 apps/copycraft/testdata/images/make_images.py

qr_code.png             QR code with the payload "Copycraft QR test 2026"
ocr_text.png            three lines of black text on white, for OCR
exif_rotated_gps.jpg    EXIF Orientation 6 (shown rotated 90 degrees clockwise) and a fake GPS
                        position (52.1 N, 5.1 E), camera "Copycraft Test Camera"
"""
from pathlib import Path

import qrcode
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"


def font(size):
    try:
        return ImageFont.truetype(FONT, size)
    except OSError:
        return ImageFont.load_default(size)


def qr_code():
    qr = qrcode.QRCode(border=4, box_size=8, error_correction=qrcode.constants.ERROR_CORRECT_M)
    qr.add_data("Copycraft QR test 2026")
    qr.make(fit=True)
    img = qr.make_image(fill_color="black", back_color="white").convert("L")
    img.save(HERE / "qr_code.png", optimize=True)


def ocr_text():
    lines = ["Copycraft OCR test", "Invoice 2026-0042", "Total 59.90 EUR"]
    img = Image.new("L", (520, 200), 255)
    draw = ImageDraw.Draw(img)
    for i, line in enumerate(lines):
        draw.text((24, 20 + i * 58), line, fill=0, font=font(40))
    img.save(HERE / "ocr_text.png", optimize=True)


def exif_rotated_gps():
    # Drawn upright (an arrow pointing up, the word TOP), then stored turned 90 degrees
    # counter-clockwise: a viewer that honours Orientation 6 turns it back upright.
    upright = Image.new("RGB", (240, 320), (235, 240, 250))
    draw = ImageDraw.Draw(upright)
    draw.polygon([(120, 30), (60, 120), (180, 120)], fill=(200, 40, 40))
    draw.rectangle([100, 120, 140, 230], fill=(200, 40, 40))
    draw.text((78, 250), "TOP", fill=(20, 20, 20), font=font(36))
    stored = upright.transpose(Image.Transpose.ROTATE_90)
    exif = Image.Exif()
    exif[0x0112] = 6  # Orientation: rotate 90 CW to display
    exif[0x010F] = "Copycraft"  # Make
    exif[0x0110] = "Copycraft Test Camera"  # Model
    exif[0x0132] = "2026:01:02 03:04:05"  # DateTime
    gps = exif.get_ifd(0x8825)
    gps[1] = "N"
    gps[2] = (52.0, 6.0, 0.0)  # 52 deg 6 min = 52.1 N
    gps[3] = "E"
    gps[4] = (5.0, 6.0, 0.0)  # 5 deg 6 min = 5.1 E
    stored.save(HERE / "exif_rotated_gps.jpg", quality=80, exif=exif)


if __name__ == "__main__":
    qr_code()
    ocr_text()
    exif_rotated_gps()
