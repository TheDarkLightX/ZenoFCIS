"""Original, licensed reading font embedded in offline presentation artifacts."""
from base64 import b64encode
from pathlib import Path

ASSET_ROOT = Path(__file__).resolve().parent / "reading_assets"
ASSETS = ("LinuxLibertine-Regular.otf", "LICENSE.txt", "SOURCE.json")


def font_css() -> str:
    """Keep the original font bytes and its readable notice in each CSS copy."""
    font = b64encode((ASSET_ROOT / ASSETS[0]).read_bytes()).decode("ascii")
    license_text = (ASSET_ROOT / "LICENSE.txt").read_text(encoding="utf-8")
    return ("/*\n" + license_text + "*/\n"
            '@font-face{font-family:"ZAL Reading";src:url("data:font/otf;base64,'
            + font + '") format("opentype");font-style:normal;font-weight:400;'
            'font-display:swap}\n')
