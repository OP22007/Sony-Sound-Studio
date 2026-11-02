"""Extract straight-line DEX enum constructor constants without executing the APK.

This bounded interpreter tracks constants and static enum references only. It
rejects branches/payloads rather than pretending to evaluate arbitrary bytecode.
Use REA class signatures to verify the constructor argument meanings separately.
"""
import argparse
import json
import struct
import zipfile


def extract(data, class_name):
    u32 = lambda offset: struct.unpack_from('<I', data, offset)[0]

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
        offset = u32(u32(60) + index * 4)
        _, offset = uleb(offset)
        strings.append(data[offset:data.index(0, offset)].decode('utf-8', errors='replace'))
    types = [strings[u32(u32(68) + index * 4)] for index in range(u32(64))]
    target = 'L' + class_name.replace('.', '/') + ';'
    if target not in types:
        return []
    fields = []
    for index in range(u32(80)):
        cls, _, name = struct.unpack_from('<HHI', data, u32(84) + index * 8)
        fields.append(types[cls] + '->' + strings[name])
    methods = []
    for index in range(u32(88)):
        cls, _, name = struct.unpack_from('<HHI', data, u32(92) + index * 8)
        methods.append((types[cls], strings[name]))
    results = []
    for index in range(u32(96)):
        class_offset = u32(100) + index * 32
        if types[u32(class_offset)] != target:
            continue
        offset = u32(class_offset + 24)
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
                if methods[method_index] != (target, '<clinit>'):
                    continue
                units = struct.unpack_from('<' + 'H' * u32(code_offset + 12), data, code_offset + 16)
                registers = {}
                pc = 0
                while pc < len(units):
                    word = units[pc]
                    op, reg = word & 255, word >> 8
                    if op == 0 or 0x28 <= op <= 0x2c or 0x32 <= op <= 0x3d:
                        raise ValueError('Control flow or payload in initializer; inspect in REA instead')
                    width = 1
                    if op in [2, 5, 8, 0x13, 0x15, 0x16, 0x19, 0x1a, 0x1c, 0x1f, 0x20, 0x22, 0x23] or 0x2d <= op <= 0x31 or 0x44 <= op <= 0x6d or 0x90 <= op <= 0xaf or 0xd0 <= op <= 0xe2 or op >= 0xfe:
                        width = 2
                    if op in [3, 6, 9, 0x14, 0x17, 0x1b, 0x24, 0x25, 0x26] or 0x6e <= op <= 0x78 or op in [0xfc, 0xfd]:
                        width = 3
                    if op == 0x18:
                        width = 5
                    if op in [0xfa, 0xfb]:
                        width = 4
                    if op == 0x12:
                        value = reg >> 4
                        registers[reg & 15] = value if value < 8 else value - 16
                    elif op == 0x13:
                        registers[reg] = struct.unpack('<h', struct.pack('<H', units[pc + 1]))[0]
                    elif op == 0x14:
                        registers[reg] = struct.unpack('<i', struct.pack('<HH', *units[pc + 1:pc + 3]))[0]
                    elif op in [0x1a, 0x1b]:
                        string_index = units[pc + 1] + (units[pc + 2] * 65536 if op == 0x1b else 0)
                        registers[reg] = strings[string_index]
                    elif op == 0x62:
                        registers[reg] = fields[units[pc + 1]]
                    elif op == 0x22:
                        registers[reg] = {'class': types[units[pc + 1]]}
                    elif op in [1, 4, 7]:
                        registers[reg & 15] = registers.get(reg >> 4)
                    elif op in [2, 5, 8]:
                        registers[reg] = registers.get(units[pc + 1])
                    elif op in [0x70, 0x76]:
                        method = methods[units[pc + 1]]
                        if op == 0x70:
                            count_args, last = reg >> 4, reg & 15
                            packed = units[pc + 2]
                            args = [(packed >> (4 * i)) & 15 for i in range(4)] + [last]
                            args = args[:count_args]
                        else:
                            args = list(range(units[pc + 2], units[pc + 2] + reg))
                        if method == (target, '<init>'):
                            values = [registers.get(argument) for argument in args]
                            results.append({'name': values[1], 'ordinal': values[2], 'arguments': values[3:], 'code_unit_offset': pc})
                    pc += width
    return results


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('apk')
    parser.add_argument('class_name')
    parser.add_argument('--match', default='')
    args = parser.parse_args()
    with zipfile.ZipFile(args.apk) as archive:
        for name in archive.namelist():
            if name.startswith('classes') and name.endswith('.dex'):
                for row in extract(archive.read(name), args.class_name):
                    if args.match.lower() in row['name'].lower():
                        print(json.dumps(dict(dex=name, **row)))
