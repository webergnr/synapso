#!/usr/bin/env python3
"""Builds a deliberately unfair vault: big notes, a huge board, many images.

    python3 tools/stress-vault.py /tmp/synapso-stress

Everything is deterministic (fixed seed), so two runs give the same vault and
timings can be compared. No third-party packages — the PNGs are written by
hand, which is why they are gradients and not photographs.
"""

import json
import os
import random
import struct
import sys
import zlib

WORDS = """lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod
tempor incididunt ut labore et dolore magna aliqua enim ad minim veniam quis
nostrud exercitation ullamco laboris nisi aliquip ex ea commodo consequat duis
aute irure in reprehenderit voluptate velit esse cillum eu fugiat nulla pariatur
excepteur sint occaecat cupidatat non proident sunt culpa qui officia deserunt
mollit anim id est laborum adapter storage queue canvas spine vault synapso
rotation viewport tessellation gradient""".split()

TOPICS = ["adapter", "storage", "queue", "canvas", "vault", "index", "render",
          "layout", "sync", "schema", "cache", "parser", "search", "atlas"]


def png(path, w, h, seed):
    """A valid RGB PNG, written without PIL. Gradient + noise so the encoder
    cannot collapse it to nothing — a 1-colour image would not stress decode."""
    rnd = random.Random(seed)
    base = (rnd.randrange(60, 220), rnd.randrange(60, 220), rnd.randrange(60, 220))
    rows = bytearray()
    for y in range(h):
        rows.append(0)  # filter: none
        row = bytearray()
        for x in range(w):
            row.append((base[0] + x * 255 // max(w, 1)) % 256)
            row.append((base[1] + y * 255 // max(h, 1)) % 256)
            row.append((base[2] + (x ^ y)) % 256)
        rows += row

    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(bytes(rows), 6)))
        f.write(chunk(b"IEND", b""))


def paragraph(rnd, n):
    words = [rnd.choice(WORDS) for _ in range(n)]
    words[0] = words[0].capitalize()
    return " ".join(words) + "."


def note(rnd, i):
    out = [f"# {TOPICS[i % len(TOPICS)]} {i}", ""]
    for section in range(rnd.randrange(6, 12)):
        out += [f"## {rnd.choice(TOPICS)} {section}", ""]
        for _ in range(rnd.randrange(3, 6)):
            out += [paragraph(rnd, rnd.randrange(60, 140)), ""]
        kind = rnd.randrange(4)
        if kind == 0:
            out += [f"- [ ] {paragraph(rnd, 8)}" for _ in range(4)] + [""]
        elif kind == 1:
            out += ["```rust", "fn " + rnd.choice(TOPICS) + "(n: usize) -> usize {",
                    "    n * 2 // " + paragraph(rnd, 5), "}", "```", ""]
        elif kind == 2:
            out += ["> " + paragraph(rnd, 20), "> " + paragraph(rnd, 18), ""]
        # every note points at two others, so mentions are exercised too
        out += [f"See [[{TOPICS[(i + 3) % len(TOPICS)]} {(i + 3) % 100}]] "
                f"and [[canvas: node-{rnd.randrange(500)}]].", ""]
    return "\n".join(out)


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "/tmp/synapso-stress"
    notes, images, nodes_n, edges_n = 100, 500, 1000, 700
    rnd = random.Random(7)
    os.makedirs(os.path.join(root, "assets"), exist_ok=True)

    names = []
    for i in range(notes):
        name = f"{TOPICS[i % len(TOPICS)]} {i}"
        names.append(name)
        with open(os.path.join(root, f"{name}.md"), "w") as f:
            f.write(note(rnd, i))

    # 400 thumbnails, 80 screenshots, 20 posters — the mix a real board grows
    sizes = [(256, 192)] * 400 + [(640, 480)] * 80 + [(1600, 1200)] * 20
    for i, (w, h) in enumerate(sizes[:images]):
        png(os.path.join(root, "assets", f"img-{i:03}.png"), w, h, i)

    shapes = ["rect", "ellipse", "triangle", "label"]
    nodes, edges = [], []
    span_x, span_y = 14000, 9000

    for i in range(nodes_n):
        x, y = rnd.randrange(-span_x, span_x), rnd.randrange(-span_y, span_y)
        roll = rnd.random()
        node = {"id": f"node-{i}", "x": x, "y": y}
        if roll < 0.25:  # doc card
            name = rnd.choice(names)
            node |= {"type": "file", "file": f"{name}.md", "width": 380, "height": 170}
            if rnd.random() < 0.4:
                node["subpath"] = f"#{rnd.choice(TOPICS)} {rnd.randrange(6)}"
        elif roll < 0.35:  # frame
            node |= {"type": "group", "width": 900, "height": 600,
                     "label": f"frame {i}"}
        else:
            node |= {"type": "text", "width": rnd.randrange(160, 420),
                     "height": rnd.randrange(90, 260),
                     "text": paragraph(rnd, rnd.randrange(6, 40)),
                     "shape": rnd.choice(shapes)}
            if rnd.random() < 0.3:
                node["fontSize"] = rnd.choice([11, 13, 18, 24, 32])
        if rnd.random() < 0.4:
            node["rotation"] = round(rnd.uniform(0.1, 359.9), 1)
        if rnd.random() < 0.5:
            node["color"] = str(rnd.randrange(1, 7))
        nodes.append(node)

    for i, (w, h) in enumerate(sizes[:images]):
        scale = 0.35
        nodes.append({
            "id": f"img-{i}", "type": "file", "file": f"assets/img-{i:03}.png",
            "x": rnd.randrange(-span_x, span_x), "y": rnd.randrange(-span_y, span_y),
            "width": int(w * scale), "height": int(h * scale),
            **({"rotation": round(rnd.uniform(0.1, 359.9), 1)} if rnd.random() < 0.3 else {}),
        })

    ids = [n["id"] for n in nodes]
    for i in range(edges_n):
        a, b = rnd.choice(ids), rnd.choice(ids)
        if a == b:
            continue
        edges.append({"id": f"edge-{i}", "fromNode": a, "toNode": b,
                      **({"toEnd": "none"} if rnd.random() < 0.3 else {}),
                      **({"label": f"link {i}"} if rnd.random() < 0.2 else {})})

    with open(os.path.join(root, "stress.canvas"), "w") as f:
        json.dump({"nodes": nodes, "edges": edges}, f)

    total = sum(os.path.getsize(os.path.join(dp, f))
                for dp, _, fs in os.walk(root) for f in fs)
    print(f"{root}: {notes} notes, {len(nodes)} nodes, {len(edges)} edges, "
          f"{images} images, {total / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
