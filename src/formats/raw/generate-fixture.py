# Recreate the original MIT-licensed sensor fixture; no camera samples.
import struct
from pathlib import Path
P = lambda fmt, *v: struct.pack('<' + fmt, *v)
tags = []

def tag(id, typ, count, data):
    tags.append((id, typ, count, data))

def shorts(id, *v):
    tag(id, 3, len(v), P('H' * len(v), *v))

def longs(id, *v):
    tag(id, 4, len(v), P('I' * len(v), *v))

def ascii(id, s):
    tag(id, 2, len(s) + 1, s.encode() + b'\x00')

def rational(id, vs, signed=False):
    tag(id, 10 if signed else 5, len(vs), b''.join((P('ii' if signed else 'II', a, b) for a, b in vs)))
w, h = (64, 48)
longs(254, 0)
longs(256, w)
longs(257, h)
shorts(258, 16)
shorts(259, 1)
shorts(262, 32803)
ascii(271, 'Omadesign')
ascii(272, 'Synthetic Bayer')
longs(273, 0)
shorts(274, 1)
shorts(277, 1)
longs(278, h)
longs(279, w * h * 2)
shorts(284, 1)
shorts(33421, 2, 2)
tag(33422, 1, 4, bytes([0, 1, 1, 2]))
rational(33434, [(1, 125)])
rational(33437, [(28, 10)])
shorts(34855, 200)
rational(37386, [(50, 1)])
tag(50706, 1, 4, bytes([1, 4, 0, 0]))
tag(50707, 1, 4, bytes([1, 1, 0, 0]))
ascii(50708, 'Omadesign Synthetic Bayer')
tag(50710, 1, 3, bytes([0, 1, 2]))
shorts(50711, 1)
shorts(50713, 1, 1)
rational(50714, [(0, 1)])
longs(50717, 65535)
rational(50718, [(1, 1), (1, 1)])
rational(50721, [(1, 1), (0, 1), (0, 1), (0, 1), (1, 1), (0, 1), (0, 1), (0, 1), (1, 1)], True)
rational(50728, [(1, 1), (1, 1), (1, 1)])
rational(50730, [(1, 1)], True)
shorts(50778, 21)
tags.sort()
offset = 8 + 2 + 12 * len(tags) + 4
payload = bytearray()
entries = []
for id, typ, count, data in tags:
    if len(data) > 4:
        entries.append((id, typ, count, P('I', offset + len(payload))))
        payload += data
        if len(payload) % 2:
            payload += b'\x00'
    else:
        entries.append((id, typ, count, data.ljust(4, b'\x00')))
raw_offset = offset + len(payload)
entries = [(id, t, c, P('I', raw_offset) if id == 273 else d) for id, t, c, d in entries]
raw = b''.join((P('H', 1000 + (x + y * w) * 18) for y in range(h) for x in range(w)))
file = b'II*\x00' + P('I', 8) + P('H', len(tags)) + b''.join((P('HHI', id, t, c) + d for id, t, c, d in entries)) + P('I', 0) + payload + raw
Path(__file__).with_name('synthetic.dng').write_bytes(file)
