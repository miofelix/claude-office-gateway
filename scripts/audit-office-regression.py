#!/usr/bin/env python3
"""Read-only structural checks for the saved live PowerPoint regression deck."""

import argparse
import hashlib
import json
from pathlib import Path
import posixpath
import sys
import xml.etree.ElementTree as ET
import zipfile

NS = {
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "c": "http://schemas.openxmlformats.org/drawingml/2006/chart",
}


def xml_hash(data):
    canonical = ET.canonicalize(data.decode(), strip_text=True)
    return hashlib.sha256(canonical.encode()).hexdigest()


def related(parts, source):
    rel = posixpath.join(posixpath.dirname(source), "_rels", posixpath.basename(source) + ".rels")
    if rel not in parts:
        return {}
    return {
        item.attrib["Id"]: posixpath.normpath(posixpath.join(posixpath.dirname(source), item.attrib["Target"]))
        for item in ET.fromstring(parts[rel])
        if item.get("TargetMode") != "External"
    }


def read_deck(path):
    with zipfile.ZipFile(path) as archive:
        bad = archive.testzip()
        if bad:
            raise ValueError("PPTX CRC verification failed")
        parts = {name: archive.read(name) for name in archive.namelist()}
    rel = related(parts, "ppt/presentation.xml")
    slide_ids = ET.fromstring(parts["ppt/presentation.xml"]).find("p:sldIdLst", NS)
    slides = []
    for item in slide_ids:
        name = rel[item.attrib["{" + NS["r"] + "}id"]]
        root = ET.fromstring(parts[name])
        texts = [x.text or "" for x in root.findall(".//a:t", NS)]
        tables = []
        for table in root.findall(".//a:tbl", NS):
            tables.append([
                ["".join(cell.itertext()).strip() for cell in row.findall("a:tc/a:txBody", NS)]
                for row in table.findall("a:tr", NS)
            ])
        charts = []
        chart_hashes = []
        slide_rel = related(parts, name)
        for chart in root.findall(".//c:chart", NS):
            chart_name = slide_rel[chart.attrib["{" + NS["r"] + "}id"]]
            chart_root = ET.fromstring(parts[chart_name])
            chart_hashes.append(xml_hash(parts[chart_name]))
            series = []
            for series_node in chart_root.findall(".//c:scatterChart/c:ser", NS):
                series.append({
                    axis: [float(value.text) for value in series_node.findall(f".//c:{axis}Val//c:pt/c:v", NS)]
                    for axis in ("x", "y")
                })
            charts.append({"scatter": bool(chart_root.findall(".//c:scatterChart", NS)), "series": series})
        shape_ids = [x.attrib["id"] for x in root.findall(".//p:cNvPr", NS)]
        slides.append({"texts": texts, "tables": tables, "charts": charts,
                       "slide_hash": xml_hash(parts[name]), "chart_hashes": chart_hashes,
                       "note_hashes": [xml_hash(parts[p]) for p in slide_rel.values() if p.startswith("ppt/notesSlides/notesSlide")],
                       "unique_shape_ids": len(shape_ids) == len(set(shape_ids))})
    return slides


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--deck", type=Path, required=True)
    parser.add_argument("--provider", action="append", required=True,
                        help="Provider prefix in the order tests appended their three pages")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    baseline, deck = read_deck(args.baseline), read_deck(args.deck)
    checks = [{"check": "page_count", "passed": len(deck) == len(baseline) + 3 * len(args.provider),
               "actual": len(deck), "expected": len(baseline) + 3 * len(args.provider)},
              {"check": "baseline_page_text_and_order", "passed": [s["texts"] for s in deck[:len(baseline)]] == [s["texts"] for s in baseline]},
              {"check": "baseline_slide_xml_chart_data_and_notes", "passed":
               [{key: s[key] for key in ("slide_hash", "chart_hashes", "note_hashes")} for s in deck[:len(baseline)]] ==
               [{key: s[key] for key in ("slide_hash", "chart_hashes", "note_hashes")} for s in baseline]},
              {"check": "unique_shape_ids", "passed": all(s["unique_shape_ids"] for s in deck)}]
    expected_table = [["方法", "输入", "输出"], ["机理", "物理定律", "动态模型"], ["学习", "数据", "预测模型"]]
    expected_series = [{"x": [0, 1, 2], "y": [0, 1, 4]}, {"x": [0, 1, 2], "y": [0, -1, -4]}]
    for group, prefix in enumerate(args.provider):
        block = deck[len(baseline) + group * 3:len(baseline) + group * 3 + 3]
        if len(block) != 3:
            checks.append({"provider": prefix, "check": "three_pages_exist", "passed": False})
            continue
        text, table, chart = block
        checks.extend([
            {"provider": prefix, "check": "read_write_final_text", "passed": "中文参数完整 42" in "".join(text["texts"])},
            {"provider": prefix, "check": "native_table_cells", "passed": table["tables"] == [expected_table], "native_table_count": len(table["tables"])},
            {"provider": prefix, "check": "one_native_xy_chart_exact_data", "passed": chart["charts"] == [{"scatter": True, "series": expected_series}], "chart_count": len(chart["charts"])},
        ])
    report = {"all_passed": all(c["passed"] for c in checks), "checks": checks}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    return 0 if report["all_passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
