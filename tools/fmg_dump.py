"""Dumps AC6 FMG files (version 2, UTF-16) as "id<TAB>text" lines.
Usage: python fmg_dump.py <fmg or folder> <output folder>"""
import os
import struct
import sys


def read_fmg(path):
    with open(path, "rb") as f:
        data = f.read()
    group_count = struct.unpack_from("<i", data, 0x0C)[0]
    offsets_at = struct.unpack_from("<q", data, 0x18)[0]
    entries = []
    for g in range(group_count):
        first_index, first_id, last_id = struct.unpack_from("<iii", data, 0x28 + g * 0x10)
        for i in range(last_id - first_id + 1):
            text_at = struct.unpack_from("<q", data, offsets_at + (first_index + i) * 8)[0]
            if text_at == 0:
                continue
            end = text_at
            while data[end:end + 2] != b"\0\0":
                end += 2
            entries.append((first_id + i, data[text_at:end].decode("utf-16-le")))
    return entries


def main():
    source, out_dir = sys.argv[1], sys.argv[2]
    os.makedirs(out_dir, exist_ok=True)
    files = [source] if os.path.isfile(source) else [os.path.join(source, f) for f in os.listdir(source) if f.endswith(".fmg")]
    for path in files:
        out = os.path.join(out_dir, os.path.basename(path) + ".tsv")
        with open(out, "w", encoding="utf-8") as f:
            for text_id, text in read_fmg(path):
                f.write(f"{text_id}\t{text.replace(chr(10), ' / ')}\n")


main()
