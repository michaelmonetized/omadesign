"""Package the recorded document's identical test font using native archive IDs."""
import json
from pathlib import Path
import shutil

root = Path(__file__).resolve().parent
fixture = root.parents[2] / 'tests/assets/fonts/EBGaramond.ttf'
font = fixture.read_bytes()
fingerprint = 0x6c62272e07bb014262b821756295c58d
for byte in font:
    fingerprint = ((fingerprint ^ byte) * 0x0000000001000000000000000000013b) & ((1 << 128) - 1)
font_id = f'omatype:{fingerprint:032x}'
archive = root / '.omabrand/fonts'
archive.mkdir(parents=True, exist_ok=True)
(archive / f'{fingerprint:032x}.ttf').write_bytes(font)
shutil.copyfile(fixture.with_name('EBGaramond-OFL.txt'), archive / 'OFL.txt')
doc = json.loads((root / 'recorded.oma').read_text())

def portable(value):
    if isinstance(value, dict):
        if 'font' in value:
            assert value['font'].endswith('/EBGaramond.ttf')
            value['font'] = font_id
        for child in value.values():
            portable(child)
    elif isinstance(value, list):
        for child in value:
            portable(child)

portable(doc)
(root / 'saved.oma').write_text(json.dumps(doc, separators=(',', ':'), ensure_ascii=False))
