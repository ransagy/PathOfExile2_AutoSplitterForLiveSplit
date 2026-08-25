#!/usr/bin/env python3
"""Generate .lss split files mirroring the C# component's "Generate" button.

Pulls the campaign area table and its canonical generation order straight from
the upstream C# sources (SavageOranges/PathOfExile2_AutoSplitterForLiveSplit),
so re-running this after syncing the fork regenerates faithful splits:

  - Campaign-100.lss         every area, AddCampaignArea() order
  - Campaign-AnyPercent.lss  mandatory-only subset, same order
  - Levels-2-100.lss         "Level 2".."Level 100"

Icons are embedded as base64 PNGs exactly like GenerateWithIcons=true does
(mapping from Properties/Resources.resx).

Usage: python3 generate.py   (from this directory)
"""

import base64
import io
import re
import urllib.request
import xml.sax.saxutils as sax
from pathlib import Path

UPSTREAM = "https://raw.githubusercontent.com/SavageOranges/PathOfExile2_AutoSplitterForLiveSplit/main"
GAME_NAME = "Path of Exile 2"
LSS_VERSION = "1.7.0"

# EnIconType -> resource name (from GetIconForType) -> PNG file (from resx)
ICON_FILES = {
    "Town": "Town_WorldMapIcon.png",
    "Waypoint": "Waypoint_WorldMapIcon.png",
    "NoWaypoint": "NoWaypoint_WorldMapIcon.png",
    "InteriorWaypoint": "UndergroundWaypoint_WorldMapIcon.png",
    "InteriorNoWaypoint": "UndergroundNoWaypoint_WorldMapIcon.png",
    "Level": "LevelUp.png",  # resource LevelUp_Icon
}


def fetch(path: str) -> str:
    with urllib.request.urlopen(f"{UPSTREAM}/{path}") as r:
        return r.read().decode("utf-8")


def parse_area_constants(areas_cs: str) -> dict[str, tuple[str, str, bool]]:
    """constantName -> (AreaName, IconType, IsMandatory)"""
    consts: dict[str, tuple[str, str, bool]] = {}
    for m in re.finditer(
        r"(\w+)\s*=\s*new CampaignAreaDto\s*\{(.*?)\n\s*\};", areas_cs, re.S
    ):
        cname, body = m.group(1), m.group(2)
        area_id = re.search(r'AreaId\s*=\s*"([^"]+)"', body).group(1)
        area_name = re.search(r'AreaName\s*=\s*"([^"]+)"', body).group(1)
        icon_m = re.search(r"IconType\s*=\s*EnIconType\.(\w+)", body)
        icon = icon_m.group(1) if icon_m else "Waypoint"  # C# default fallback
        mandatory = bool(re.search(r"IsMandatory\s*=\s*true", body))
        consts[cname] = (area_name, icon, mandatory)
        assert area_id, cname
    return consts


def parse_generation_order(campaign_area_cs: str) -> list[str]:
    return re.findall(r"AddCampaignArea\(CampaignAreas\.(\w+)\)", campaign_area_cs)


def load_icon_b64(png_name: str) -> str:
    cache = Path(__file__).parent / ".icon-cache"
    cache.mkdir(exist_ok=True)
    f = cache / png_name
    if not f.exists():
        data = urllib.request.urlopen(f"{UPSTREAM}/Resources/{png_name}").read()
        f.write_bytes(data)
    return base64.b64encode(f.read_bytes()).decode("ascii")


def write_lss(
    path: Path,
    category: str,
    segments: list[tuple[str, str]],
    icons: dict[str, str],
) -> None:
    out = io.StringIO()
    w = out.write
    w('<?xml version="1.0" encoding="UTF-8"?>\n')
    w(f'<Run version="{LSS_VERSION}">\n')
    w("  <GameIcon />\n")
    w("  <PlatformName />\n")
    w(f"  <GameName>{sax.escape(GAME_NAME)}</GameName>\n")
    w(f"  <CategoryName>{sax.escape(category)}</CategoryName>\n")
    w("  <Metadata>\n")
    w('    <Run id="" />\n')
    w("    <Variables />\n")
    w("  </Metadata>\n")
    w("  <Offset>00:00:00</Offset>\n")
    w("  <AttemptCount>0</AttemptCount>\n")
    w("  <AttemptHistory />\n")
    w("  <Segments>\n")
    for name, icon in segments:
        w("    <Segment>\n")
        w(f"      <Name>{sax.escape(name)}</Name>\n")
        if icon:
            w(f"      <IconImage>{icons[icon]}</IconImage>\n")
        else:
            w("      <IconImage />\n")
        w("    </Segment>\n")
    w("  </Segments>\n")
    w("  <AutoSplitterSettings />\n")
    w("</Run>\n")
    path.write_text(out.getvalue(), encoding="utf-8")
    print(f"wrote {path} ({len(segments)} segments)")


def main() -> None:
    areas_cs = fetch("Component/Constants/CampaignAreas.cs")
    order_cs = fetch("Component/Timer/CampaignArea.cs")

    consts = parse_area_constants(areas_cs)
    order = parse_generation_order(order_cs)
    missing = [c for c in order if c not in consts]
    if missing:
        raise SystemExit(f"constants referenced but not defined: {missing}")

    full = [consts[c] for c in order]
    any_percent = [a for a in full if a[2]]
    levels = [(f"Level {i}", "Level", False) for i in range(2, 101)]

    needed_icons = {icon for _, icon, _ in full + levels}
    icons = {k: load_icon_b64(ICON_FILES[k]) for k in sorted(needed_icons)}

    out_dir = Path(__file__).parent

    def seg(name: str, icon: str) -> tuple[str, str]:
        return name, icon

    write_lss(
        out_dir / "Campaign-100.lss",
        "Campaign (100%)",
        [seg(n, i) for n, i, _ in full],
        icons,
    )
    write_lss(
        out_dir / "Campaign-AnyPercent.lss",
        "Campaign (Any%)",
        [seg(n, i) for n, i, _ in any_percent],
        icons,
    )
    write_lss(
        out_dir / "Levels-2-100.lss",
        "Level",
        [seg(n, i) for n, i, _ in levels],
        icons,
    )


if __name__ == "__main__":
    main()
