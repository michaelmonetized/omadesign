"""Read DRM driver and reported GPU-memory allocation without privileged identifiers."""
import json
import pathlib
import time

cards = []
for card in pathlib.Path('/sys/class/drm').glob('card[0-9]*'):
    if '-' in card.name:
        continue
    device = card / 'device'
    driver = device / 'driver'
    data = {'card': card.name, 'driver': driver.resolve().name if driver.exists() else None}
    for name in ['mem_info_vram_total', 'mem_info_vis_vram_total', 'mem_info_gtt_total']:
        try:
            data[name + '_bytes'] = int((device / name).read_text().strip())
        except (OSError, ValueError):
            pass
    cards.append(data)
print(json.dumps({'captured_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'cards': cards}, indent=2))
