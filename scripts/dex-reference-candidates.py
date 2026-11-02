"""Find candidate DEX static-field readers for follow-up static inspection.

This scans aligned code units, not decoded instructions. Results are candidates
only and may include operand false positives. Validate every result in REA.
"""

import argparse
import struct
import zipfile


def scan(data, field_name):
    def u32(offset):
        return struct.unpack_from("<I", data, offset)[0]

    def uleb(offset):
        value = shift = 0
        while True:
            byte = data[offset]
            offset += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value, offset
            shift += 7

    strings = []
    for index in range(u32(56)):
        start = u32(u32(60) + index * 4)
        _, start = uleb(start)
        strings.append(data[start:data.index(0, start)].decode("utf-8", errors="replace"))
    types = [strings[u32(u32(68) + i * 4)] for i in range(u32(64))]
    fields = {}
    for index in range(u32(80)):
        cls, _, name = struct.unpack_from("<HHI", data, u32(84) + index * 8)
        if strings[name] == field_name:
            fields[index] = types[cls] + "->" + strings[name]
    methods = []
    for index in range(u32(88)):
        cls, _, name = struct.unpack_from("<HHI", data, u32(92) + index * 8)
        methods.append((types[cls], strings[name]))
    for i in range(u32(96)):
        offset = u32(u32(100) + i * 32 + 24)
        if not offset:
            continue
        counts = []
        for _ in range(4):
            value, offset = uleb(offset)
            counts.append(value)
        for _ in range(counts[0] + counts[1]):
            _, offset = uleb(offset)
            _, offset = uleb(offset)
        for count in counts[2:]:
            method_index = 0
            for _ in range(count):
                delta, offset = uleb(offset)
                method_index += delta
                _, offset = uleb(offset)
                code_offset, offset = uleb(offset)
                if not code_offset:
                    continue
                units = struct.unpack_from("<" + "H" * u32(code_offset + 12), data, code_offset + 16)
                for pos in range(len(units) - 1):
                    if units[pos] & 255 == 0x62 and units[pos + 1] in fields:
                        cls, name = methods[method_index]
                        print(cls[1:-1].replace("/", "."), name, "field", fields[units[pos + 1]], "code_unit", pos)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk")
    parser.add_argument("field")
    args = parser.parse_args()
    with zipfile.ZipFile(args.apk) as archive:
        for name in archive.namelist():
            if name.startswith("classes") and name.endswith(".dex"):
                scan(archive.read(name), args.field)
