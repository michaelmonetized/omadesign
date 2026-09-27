"""Keep native shapes editable while grouping SVG leaf layers by artwork section."""
from pathlib import Path
import copy,json
root=Path(__file__).parent
for stem in ['infographic-reference','infographic-seed']:
    p=root/(stem+'.oma'); value=json.loads(p.read_text());doc=value['doc']
    layers=doc['layers'];byid={l['id']:l for l in layers}
    buckets={};order=[]
    for layer in layers:
        top=layer
        while top.get('parent') is not None: top=byid[top['parent']]
        if top['id'] not in buckets:
            result=copy.deepcopy(top)
            assert result.get('mask') is None and not result['filters']['items']
            result.update({'is_group':False,'parent':None,'kind':{'Vector':{'shapes':[]}}})
            buckets[top['id']]=result;order.append(top['id'])
        assert 'Vector' in layer['kind']
        assert layer.get('mask') is None and layer['opacity']==1 and not layer['filters']['items']
        buckets[top['id']]['kind']['Vector']['shapes'].extend(layer['kind']['Vector']['shapes'])
    # Top-level standalone glints stay editable but share one sensible layer.
    compact=[]
    for key in order:
        layer=buckets[key]
        if layer['name'].startswith('Pen path') and compact and compact[-1]['name']=='Sunlight accents':
            compact[-1]['kind']['Vector']['shapes']+=layer['kind']['Vector']['shapes']
        else:
            if layer['name'].startswith('Pen path'):layer['name']='Sunlight accents'
            compact.append(layer)
    before=sum(len(l['kind']['Vector']['shapes']) for l in layers)
    after=sum(len(l['kind']['Vector']['shapes']) for l in compact)
    assert before==after
    doc['layers']=compact;doc['name']='SEOFOMO — weekly SEO + AI update'
    p.write_text(json.dumps(value,separators=(',',':')))
    print(stem,len(compact),'layers',after,'editable shapes')
