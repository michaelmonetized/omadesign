#!/usr/bin/env python3
"""Publish web branding from the user-maintained transparent assets.

Never writes assets/ or derives app artwork from an older media/logo.svg.
Only the web SVG viewports are cropped; paths and colors remain unchanged.
Requires the Omadesign CLI, Pillow (alpha bounds only), and ffmpeg.
"""
from pathlib import Path
import argparse, hashlib, json, re, shutil, subprocess, tempfile
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--renderer', default='omadesign')
args = parser.parse_args()
version = re.search(r'^version = "([^"]+)"', (ROOT/'Cargo.toml').read_text(), re.M)[1]
out = ROOT/'site/public/media/branding'
out.mkdir(parents=True, exist_ok=True)
records = []
with tempfile.TemporaryDirectory(prefix='omadesign-brand-') as temp:
    temp = Path(temp)
    for source, name in [('omadesign.svg', 'logo'), ('omadesign-wordmark.svg', 'wordmark')]:
        original = ROOT/'assets'/source
        png = temp/f'{name}.png'
        subprocess.run([args.renderer, '--convert', str(original), '--output', str(png)], check=True)
        with Image.open(png) as image:
            box = image.getchannel('A').getbbox()
            if not box: raise SystemExit(f'{source} is empty')
            x,y,right,bottom = box
        artwork = original.read_text()
        root = re.search(r'<svg\b[^>]*>', artwork)
        opening = root.group()
        for key,value in [('width',right-x),('height',bottom-y),('viewBox',f'{x} {y} {right-x} {bottom-y}')]:
            opening,count = re.subn(rf'\b{key}="[^"]*"',f'{key}="{value}"',opening)
            if count != 1: raise SystemExit(f'Missing {key} in {source}')
        output = out/f'{name}-{version}.svg'
        output.write_text(artwork[:root.start()]+opening+artwork[root.end():])
        records.append({'source':f'assets/{source}','sourceSha256':hashlib.sha256(original.read_bytes()).hexdigest(),'webFile':output.name,'viewport':[x,y,right-x,bottom-y]})
        if name == 'logo':
            subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-f','lavfi','-i','color=c=0x11111b:s=1200x630:r=1','-i',str(png),'-filter_complex',f'[1:v]crop={right-x}:{bottom-y}:{x}:{y},scale=1100:550:force_original_aspect_ratio=decrease[mark];[0:v][mark]overlay=(W-w)/2:(H-h)/2:format=auto,format=rgb24','-frames:v','1',str(out/f'logo-{version}-social.png')],check=True)
    shutil.copyfile(ROOT/'assets/omadesign.svg',out/f'icon-{version}.svg')
files=[{'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.glob(f'*-{version}*'))]
(out/'manifest.json').write_text(json.dumps({'version':version,'sources':records,'method':'Alpha-bounds viewport crop only; source SVG artwork preserved. Social render uses a dark background.','files':files},indent=2)+'\n')
