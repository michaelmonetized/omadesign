"""Editable vector reconstruction of the user-supplied SEOFOMO reference.

The seed omits the hero headline, added later with native Type-tool events.
No reference pixels are embedded in the deliverable.
"""
from pathlib import Path
from html import escape
from PIL import ImageFont
import json, math, random

ROOT=Path(__file__).parent
parts=[]; serial=0
def add(s): parts.append(s)
def ident(name):
    global serial
    serial+=1
    return escape(f'{name}-{serial}')
def rect(x,y,w,h,fill,rx=0,stroke='none',sw=1,name='Rectangle'):
    add(f'<rect id="{ident(name)}" x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>')
def ellipse(x,y,rx,ry,fill,stroke='none',sw=1,name='Ellipse'):
    add(f'<ellipse id="{ident(name)}" cx="{x}" cy="{y}" rx="{rx}" ry="{ry}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>')
def path(d,fill='none',stroke='none',sw=1,name='Pen path',opacity=1):
    add(f'<path id="{ident(name)}" d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round" opacity="{opacity}"/>')
def text(x,y,content,size=20,weight=400,color='#101115',anchor='start',name='Live text'):
    family='Noto Sans' if name=='Hero title' else 'NotoSans NFP'
    stretch='normal' if name=='Hero title' else 'condensed'
    add(f'<text id="{ident(name)}" x="{x}" y="{y}" font-family="{family}" font-stretch="{stretch}" font-size="{size}" font-weight="{weight}" fill="{color}" text-anchor="{anchor}">{escape(content)}</text>')
def lines(x,y,content,size=18,leading=21,weight=400,color='#151515'):
    for i,line in enumerate(content): text(x,y+i*leading,line,size,weight,color)
def group(name,transform=''): add(f'<g id="{ident(name)}" transform="{transform}">')
def end(): add('</g>')
def gmark(x,y,r=23):
    group('Google multicolor mark',f'translate({x} {y})')
    for a,b,c in [(-42,-137,'#ea4335'),(-137,-200,'#fbbc05'),(-200,-286,'#34a853'),(-286,-358,'#4285f4')]:
        a,b=math.radians(a),math.radians(b)
        path(f'M {r*math.cos(a)} {r*math.sin(a)} A {r} {r} 0 0 0 {r*math.cos(b)} {r*math.sin(b)}',stroke=c,sw=r*.32)
    path(f'M {r} 0 H 0',stroke='#4285f4',sw=r*.32)
    end()
def star(x,y,r=12,color='#ffce00'):
    path(f'M{x} {y-r} Q{x+2} {y-2} {x+r} {y} Q{x+2} {y+2} {x} {y+r} Q{x-2} {y+2} {x-r} {y} Q{x-2} {y-2} {x} {y-r}Z',color)
def browser(x,y,w=145,h=132):
    rect(x+3,y+4,w,h,'#dae4f3',13)
    rect(x,y,w,h,'#f8fcff',12,'#b7ccef',3)
    path(f'M{x+12} {y} H{x+w-12} Q{x+w} {y} {x+w} {y+12} V{y+25} H{x} V{y+12} Q{x} {y} {x+12} {y}Z','url(#blue)')
    for i,c in enumerate(['#ff755a','#ffd746','#a5e4ff']): ellipse(x+13+i*14,y+12,3.5,3.5,c)
    for i,width in enumerate([w-30,w-45,w-57]): rect(x+12,y+h-42+i*12,width,5,'#bdcbec',2)
def no_bug(x,y,r=41):
    ellipse(x,y,r,r,'white','#e21d24',8)
    ellipse(x,y,12,22,'#101115')
    ellipse(x,y-21,9,8,'#101115')
    for side in [-1,1]:
        for d in [-11,0,11]: path(f'M{x+side*8} {y+d} L{x+side*22} {y+d-5} L{x+side*26} {y+d+3}',stroke='#101115',sw=4)
    path(f'M{x-10} {y-25} l-7 -10 M{x+8} {y-24} l7 -10',stroke='#101115',sw=3)
    path(f'M{x-r*.68} {y-r*.69} L{x+r*.68} {y+r*.69}',stroke='white',sw=13)
    path(f'M{x-r*.68} {y-r*.69} L{x+r*.68} {y+r*.69}',stroke='#f02027',sw=8)
def spam(x,y,s=1,rotation=12):
    group('Spam paper character',f'translate({x} {y}) scale({s})')
    path('M-30 -39 H17 L34 -22 V38 H-30Z','#fffaf4','#dfd5cf',2)
    path('M17 -39 V-22 H34','#ddd5d2')
    rect(-26,-29,51,22,'#ef2c2c',1)
    text(0,-13,'SPAM',15,900,'white','middle')
    path('M-15 -1 l9 5 M8 3 l10 -6 M-6 16 Q3 7 11 16',stroke='#161820',sw=3)
    path('M-28 9 l-15 -10 l-4 6 M32 7 l12 8 l5 -4 M-18 39 l-7 12 l-8 -2 M18 39 l6 10 l10 -2',stroke='#172235',sw=5)
    end()

defs='''<defs>
<linearGradient id="sky" x2="0" y2="1"><stop stop-color="#43baff"/><stop offset="1" stop-color="#dcfaff"/></linearGradient>
<linearGradient id="water" x2="0" y2="1"><stop stop-color="#43cee9"/><stop offset="1" stop-color="#c9faff"/></linearGradient>
<linearGradient id="blue" x2="0.4" y2="1"><stop stop-color="#1896ff"/><stop offset="0.45" stop-color="#0758f7"/><stop offset="1" stop-color="#00219f"/></linearGradient>
<linearGradient id="gold" x2="0.5" y2="1"><stop stop-color="#fff976"/><stop offset="0.5" stop-color="#ffc800"/><stop offset="1" stop-color="#eda100"/></linearGradient>
<linearGradient id="robot" x2="1" y2="0.5"><stop stop-color="#e4eafa"/><stop offset="0.3" stop-color="#ffffff"/><stop offset="0.8" stop-color="#ffffff"/><stop offset="1" stop-color="#d9e6ff"/></linearGradient>
<linearGradient id="whitefade"><stop stop-color="white"/><stop offset="0.62" stop-color="white" stop-opacity="0.94"/><stop offset="1" stop-color="white" stop-opacity="0"/></linearGradient>
</defs>'''

rect(0,0,1200,900,'white',name='Paper')
group('Coastal landscape')
rect(0,0,1200,390,'url(#sky)')
for x,y,s in [(480,75,1),(882,49,.8),(1109,9,1.3),(115,25,.8),(701,57,.5)]:
    for dx,dy,r in [(-26,8,19),(0,0,26),(27,12,18),(47,18,15)]: ellipse(x+dx*s,y+dy*s,r*s,r*.6*s,'#f4fdff')
path('M260 284 Q425 195 511 238 Q613 138 696 214 Q742 173 811 232 L970 297Z','#8fcada')
path('M411 295 Q496 246 552 268 Q633 205 714 267 L874 318Z','#aacbd5')
rect(0,280,1200,112,'url(#water)')
for i in range(23):
    y=292+i*4
    path(f'M{40+i%6*29} {y} Q370 {y-3} {641+i%3*38} {y+1}',stroke='#dcffff',sw=1.5)
path('M604 389 Q651 347 729 333 Q862 360 997 257 Q1095 208 1200 122 V390Z','#f6d9a6')
path('M920 346 Q1030 282 1061 221 L1200 145 V390H996Z','#ffefcf')
random.seed(23)
for i in range(46):
    x=random.uniform(1020,1200); y=290-(x-1020)*.63+random.uniform(-45,15)
    w=random.uniform(10,21); h=random.uniform(15,34)
    rect(round(x,1),round(y,1),round(w,1),round(h,1),random.choice(['#fffdf8','#fff3cf','#ffd9be','#f5f5ff']),1)
    path(f'M{x-1} {y} L{x+w/2} {y-6} L{x+w+1} {y}Z',random.choice(['#ec6650','#32a7dc','#f5bb63']))
    rect(x+4,y+8,4,7,'#498cbb',1)
for x,y in [(585,358),(566,369),(991,358),(1055,352),(1177,281),(1190,117),(1004,324)]:
    for dx in [-8,0,8]: path(f'M{x} {y} Q{x+dx} {y-21} {x+dx*2} {y-30} Q{x+dx+7} {y-10} {x} {y}Z','#56a847')
rect(0,75,615,313,'url(#whitefade)')
end()

group('Newsletter banner')
rect(24,20,334,55,'#0739eb',17)
rect(40,35,34,27,'none',2,'white',3)
path('M41 37 L57 50 L73 37 M41 60 L53 48 M62 48 L74 60',stroke='white',sw=2.5)
text(91,57,'SEOFOMO NEWSLETTER',23,700,'white')
rect(369,20,317,55,'#ffdd05',17)
rect(387,36,29,26,'none',4,'#101115',3)
path('M387 43H415 M394 31V39 M409 31V39',stroke='#101115',sw=3)
for y in [49,55]:
    for x in [394,401,408]: rect(x-1,y-1,3,3,'#101115',.5)
text(432,57,'September 27th, 2026',24,800)
end()

group('Anti-spam mascot')
ellipse(714,366,117,15,'#d6bd8d')
path('M647 311 L637 350 Q616 354 632 370 Q650 378 672 361 L677 315Z','url(#blue)','#001571',3)
path('M742 314 L761 345 Q785 341 797 358 Q786 372 765 369 L730 323Z','url(#blue)','#001571',3)
path('M652 151 Q728 121 766 169 Q793 213 784 285 Q778 329 726 331 H669 Q619 325 620 281 Q613 231 633 180Z','url(#robot)','#10216e',4)
path('M771 208 Q795 184 800 166',stroke='#142467',sw=8)
ellipse(799,172,12,14,'white','#10216e',3)
path('M624 235 L600 246',stroke='#182067',sw=8)
ellipse(601,245,12,12,'white','#182067',3)
ellipse(682,199,22,26,'white','#11131d',5)
ellipse(740,185,21,26,'white','#11131d',5)
path('M703 193 Q710 183 719 186',stroke='#151722',sw=4)
ellipse(686,201,7,11,'#090b0e')
ellipse(744,186,7,11,'#090b0e')
path('M670 166 Q681 153 695 159 M723 148 Q735 140 749 146',stroke='#11131d',sw=4)
path('M708 223 Q722 235 737 216 Q739 248 720 248 Q708 244 708 223Z','#19152b')
ellipse(725,240,9,7,'#f44970')
for x,y in [(654,220),(660,218),(755,216),(761,213)]: path(f'M{x} {y} l3 9',stroke='#ffb5d5',sw=3)
path('M619 161 Q628 105 678 95 Q728 83 746 127 L755 150Z','url(#gold)','#da9910',3)
path('M658 145 Q656 112 680 98 M708 145 Q711 105 694 98',stroke='#fff280',sw=4)
path('M612 167 Q674 132 758 143 L765 155 Q678 152 609 179Z','url(#gold)','#d89800',3)
ellipse(698,121,18,18,'white');gmark(698,121,12)
path('M801 158 L817 119',stroke='#12234f',sw=7)
no_bug(821,94,47)
path('M602 210 L663 230 L672 283 Q668 330 627 351 Q588 337 576 301 L559 248Z','#fbffff','#14286f',4)
path('M600 221 L653 238 L661 283 Q655 320 627 337 Q599 323 589 296 L571 253Z','url(#blue)','#a4caff',3)
text(616,262,'ANTI',25,900,'white','middle');text(622,289,'SPAM',25,900,'white','middle')
path('M611 308 l10 9 l18 -22',stroke='white',sw=6)
end()

group('Search telescope')
ellipse(950,353,84,11,'#d9bf8e')
path('M952 232 V300 L911 343 M952 294 L994 340 M952 301 V350',stroke='#04137b',sw=17)
path('M952 232 V300 L912 343 M952 294 L994 340 M952 301 V350',stroke='#1676f9',sw=8)
for x,y in [(911,344),(953,350),(995,343)]: ellipse(x,y,14,16,'url(#blue)','#05187e',4)
group('Telescope body','translate(931 177) rotate(-18)')
rect(-88,-43,159,86,'url(#blue)',29,'#032a8c',4)
for x in [-65,-42,34,50]: path(f'M{x} -38 V38',stroke='#1e7efe',sw=3)
ellipse(70,0,30,45,'#ffcb32','#c29230',3)
ellipse(71,0,23,38,'#fffcac')
ellipse(75,-3,16,31,'#fffef4')
ellipse(-17,34,13,13,'#0050e2','#052680',3)
ellipse(-17,32,6,6,'#299bff')
end();text(855,201,'SEOFOMO',22,800,'white');end()
spam(1082,66,.95,15);spam(1145,165,.95,16);spam(1143,278,1.06,20);spam(855,321,.97,18)
for x,y,r in [(1008,84,13),(1051,210,12),(838,261,12),(491,265,10),(509,298,8),(1170,369,10)]: star(x,y,r)

cards=[
(['Google September','Spam Update'],['Google released the','September 2026 spam','update on Sept 24. It rolls out','globally over a couple of weeks','and targets sites that abuse',"Google’s spam policies."]),
(['Search Console Adds','Multimodal Reporting'],['Google introduced web','multimodal Search reporting','and Generative AI feature','reports, covering Lens, Circle','to Search, image uploads','and image search actions.']),
(['Lighthouse Gets','AI Agent Audit'],['Lighthouse 13.5 adds an audit','for Agentic Resource','Discovery, helping assess','how AI agents can find an',"organization’s tools and",'services.']),
(['AI Checkout Expands','for Shopify Merchants'],['Eligible Shopify products in','Merchant Center can become','automatically available for','native checkout in AI Mode','and Gemini with no extra','setup.']),
(['Clarity Improves','Bot Activity Views'],['Microsoft Clarity now adds','page classification in Bot','Activity, grouping pages by','type and allowing custom','categories to compare bot','visits across sections.']),
(['Study: AI Mode Cuts','Clicks'],['A 1,100-person experiment','found fewer searches leading','to external websites when','using Google AI Mode, with','satisfaction also dropping','versus normal Google use.'])]
for i,(title,body) in enumerate(cards):
    col=i%3; row=i//3; x=16+col*397; y=390+row*211
    group('Update card '+str(i+1))
    rect(x,y,394,204,'#ffffff',14,'#dce6f1',1.5)
    ellipse(x+28,y+29,20,20,'#063afa');text(x+28,y+39,str(i+1),29,900,'white','middle')
    title_x=x+189 if col==0 else x+174
    lines(title_x,y+33,title,21,25,800)
    lines(title_x,y+83,body,16.8,20,400)
    ix=x+(27 if col==0 else 15); iy=y+67
    group('Card illustration',f'translate({ix} {iy})')
    if i==0:
        group('Browser and spam','rotate(-5 72 61)');browser(0,-18,145,145);gmark(38,44,22);no_bug(107,43,31);end()
    elif i==1:
        group('Multimodal search','rotate(-5 72 61)');browser(0,-17,141,145)
        rect(9,25,44,44,'#81c6ff',6,'white',3);ellipse(43,34,6,6,'white');path('M12 63L28 42L45 58L53 48V66Z','#3eb864')
        rect(78,26,42,34,'#131729',6);rect(88,20,21,8,'#131729',3);ellipse(99,42,10,10,'#e0ecff')
        ellipse(70,84,24,24,'#6031e5','#b79cff',3);ellipse(65,79,9,9,'none','white',3);path('M71 86l10 10',stroke='white',sw=4)
        rect(105,72,40,42,'#177aff',9,'#add8ff',3);path('M114 82h21 M114 89h16 M116 97h12l9 12',stroke='white',sw=3)
        ellipse(119,-7,25,25,'white','#e0eaff',2);gmark(119,-7,17);end()
    elif i==2:
        browser(0,-15,147,143)
        path('M82 113L91 8H124L137 113Z','#fafafa','#bfd4eb',2)
        path('M87 39L128 17L131 42L85 66Z','#fa4338');path('M84 91L132 63L135 83L81 113Z','#fa4338')
        rect(86,4,43,23,'#ffe161',3,'#cc8722',2)
        path('M84 5L106 -27L132 5Z','#fa392f')
        for x in [96,107,118]: path(f'M{x} 7V23',stroke='white',sw=3)
        rect(12,60,65,60,'#7624ee',14,'#aa82ff',3);text(44,102,'AI',36,900,'white','middle')
        for j in range(4): path(f'M{23+j*14} 54V66 M{23+j*14} 115V126',stroke='#7928e9',sw=8)
    elif i==3:
        browser(0,34,150,91)
        path('M39 -19L91 -11L104 38L29 42Z','#76b638','#4e9026',3)
        path('M48 -10Q51 -49 70 -39Q88 -34 83 -12 M58 -15Q62 -48 80 -36',stroke='#58962e',sw=3)
        text(63,27,'S',47,900,'white','middle')
        path('M17 57H49L43 83H23L17 51H9 M26 95h1 M40 95h1',stroke='#7191b6',sw=5)
        rect(20,96,104,30,'url(#blue)',9);text(72,117,'Checkout',16,700,'white','middle')
        rect(92,31,59,45,'url(#blue)',12,'#b5ddff',3);text(121,62,'AI',27,800,'white','middle')
        star(130,-3,12);star(89,68,9);path('M128 113L145 143L147 130L161 126Z','#101725','white',3)
    elif i==4:
        browser(0,15,151,115)
        path('M47 -1L72 -43Q77 -50 82 -42L109 0Q111 7 104 8H53Q44 7 47 -1Z','#007ae6')
        path('M72 -43L69 -11L107 2Z','#4bafff');path('M46 3L70 -13L75 8Z','#1656db')
        rect(0,40,61,43,'url(#blue)',14,'#75beff',3);rect(8,47,45,29,'#092956',12);ellipse(21,61,4,5,'white');ellipse(39,61,4,5,'white')
        path('M30 40V22',stroke='#064be2',sw=4);ellipse(30,19,6,6,'#0766ff')
        for j in range(4): rect(7+j*15,118-j*9,11,10+j*9,'url(#blue)',2)
        for j,c in enumerate(['#35bb78','#617cf6','#ff9e2c','#40cf95']): ellipse(83,54+j*19,4,4,c);rect(93,50+j*19,42,6,'#bdcce3',3)
    else:
        group('AI mode clicks');browser(0,4,150,127)
        ellipse(52,17,24,24,'white','#d7e4fa',2);gmark(52,17,17)
        rect(80,0,69,27,'white',12,'#ccddfa',2);text(114,19,'AI Mode',13,700,'#063ac4','middle')
        for j,h in enumerate([54,42,25,15]): rect(15+j*22,114-h,16,h,'#fc4333' if j else '#187dff',3)
        path('M82 41l9 29l14 -14 M91 70L77 57',stroke='#fb3932',sw=6)
        ellipse(128,65,22,22,'#ffcf1a','#e7a92c',2);ellipse(122,60,2.5,5,'#20221c');ellipse(136,60,2.5,5,'#20221c');path('M121 78Q127 66 137 76',stroke='#252620',sw=3)
        path('M120 97L133 123L137 114L149 111Z','#17243a','white',3);end()
    end();end()

group('Footer subscribe and links')
rect(16,811,1178,80,'white',13,'#dce6f1',1.5)
rect(63,837,90,44,'url(#blue)',5)
rect(76,821,66,48,'white',5,'#142238',3)
for y in [832,840,848]: path(f'M86 {y}H131',stroke='#bec7d5',sw=2)
path('M64 838L106 865L152 838V881H64Z','#0445dc','#0043bf',2)
path('M64 881L96 856 M152 881L119 856',stroke='#1687ff',sw=2)
ellipse(148,869,19,19,'#ffda08');path('M139 864Q145 856 148 864Q155 856 158 864Q158 871 148 878Q139 871 139 864Z','#152642')
text(174,844,'Get these updates in your inbox every week!',24,800)
text(175,876,'Join 46.3K+ SEOs at',21)
rect(368,851,254,34,'#063bf0',17);text(495,876,'SEOFOMO.CO',23,800,'white','middle')
for x in [673,855,1024]: path(f'M{x} 823V879',stroke='#c2d4e9',sw=2)
rect(706,837,42,36,'none',5,'#0e1520',4);path('M714 837V826H738V837 M707 847H747',stroke='#0e1520',sw=3)
for y in [854,865]:
    for x in [716,727,738]: ellipse(x,y,2,2,'#053cea')
text(759,857,'SEO Jobs',21)
path('M877 838L922 825L932 857L886 870L883 859Q892 853 880 850Z','white','#111c2d',3)
star(904,847,9,'#0644e7');text(943,857,'Events',21)
path('M1060 868L1082 846Q1081 829 1095 825L1090 837L1099 842L1108 834Q1114 850 1096 853L1076 875Z','white','#111c2d',4)
ellipse(1068,869,3,3,'#0939e8');text(1116,857,'Tools',21)
end()

body='\n'.join(parts)
head='<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="900" viewBox="0 0 1200 900">\n'+defs
ROOT.joinpath('infographic-seed.svg').write_text(head+body+'\n</svg>')
parts=[]
titles=[(24,154,'Your SEO + AI',78,900,'#030404',550),(24,223,'Search Updates',72,900,'#030404',562),(24,299,'of the Week',78,900,'#073cf5',487),(26,343,'The biggest updates powering',27,800,'#080c12',476),(26,374,'search this week.',27,800,'#080c12',476)]
tasks=[]
for x,y,value,size,weight,color,maxw in titles:
    file='/usr/share/fonts/noto/NotoSans-Black.ttf' if weight==900 else '/usr/share/fonts/noto/NotoSans-Bold.ttf'
    font=ImageFont.truetype(file,round(size))
    size=min(size,size*maxw/font.getlength(value))
    text(x,y,value,size,weight,color,name='Hero title')
    tasks.append({'x':x,'y':y,'text':value,'size':size,'font':file,'color':color})
ROOT.joinpath('infographic-reference.svg').write_text(head+body+'\n'+'\n'.join(parts)+'\n</svg>')
ROOT.joinpath('type-tasks.json').write_text(json.dumps(tasks,indent=2))
print('Created editable source with',serial,'objects and',len(tasks),'native Type-tool tasks')
