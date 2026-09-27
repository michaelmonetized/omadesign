"""Archive the fonts used by this artwork using Omadesign's observed TypeKit schema."""
from pathlib import Path
import json, shutil
root=Path(__file__).parent
files=[root/'infographic-reference.oma',root/'infographic-seed.oma',root/'type-tasks.json']
values=[json.loads(p.read_text()) for p in files]
fonts={}
def visit(value, replace=False):
    if isinstance(value,list):
        for item in value:visit(item,replace)
    elif isinstance(value,dict):
        for key,item in value.items():
            if key=='font' and isinstance(item,str) and item.startswith('/usr/share/fonts/'):
                if replace:value[key]=fonts[item]
                elif item not in fonts:
                    data=Path(item).read_bytes();h=0x6c62272e07bb014262b821756295c58d
                    for byte in data:h=((h^byte)*0x0000000001000000000000000000013b)&((1<<128)-1)
                    fonts[item]=f'omatype:{h:032x}'
            else:visit(item,replace)
for value in values:visit(value)
bank=root/'.omabrand';(bank/'fonts').mkdir(parents=True,exist_ok=True)
roles=[]
for path,font in fonts.items():
    dest=f'fonts/{font.split(":")[1]}.ttf'
    shutil.copyfile(path,bank/dest)
    roles.append({'name':Path(path).stem,'font':dest})
if roles:
    (bank/'.omatype').write_text(json.dumps({'version':1,'name':'SEOFOMO recreation typography','roles':roles},indent=2))
    for value,path in zip(values,files):
        visit(value,True);path.write_text(json.dumps(value,separators=(',',':')))
for name,path in [('Noto-OFL.txt','/usr/share/licenses/noto-fonts/LICENSE'),('Nerd-Fonts-OFL.txt','/usr/share/licenses/ttf-noto-nerd/LICENSE_OFL.txt')]:
    shutil.copyfile(path,bank/'fonts'/name)
print(len(fonts),'portable fonts')
