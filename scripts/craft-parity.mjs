#!/usr/bin/env node
/** Inventory pinned reference commands and pane sources. Args: reference-repo folder and JSON output. Returns source evidence, never a parity score. */
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';

const [referenceRoot,output]=process.argv.slice(2);
if(!referenceRoot||!output)throw Error('Usage: craft-parity.mjs REFERENCE_REPOS OUTPUT_JSON');
const products=['vectorcraft','photocraft','figcraft','lightcraft','designcraft','effectcraft','AniCraft','filmcraft','soundcraft'];
const walk=dir=>fs.existsSync(dir)?fs.readdirSync(dir,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?walk(path.join(dir,entry.name)):entry.isFile()&&entry.name.endsWith('.rs')?[path.join(dir,entry.name)]:[]):[];
const sha=data=>createHash('sha256').update(data).digest('hex');
const catalog=[];
for(const product of products){
  const root=path.join(referenceRoot,product);
  if(!fs.existsSync(root)){catalog.push({product,state:'reference-missing'});continue;}
  let commit;
  try{commit=execFileSync('git',['-C',root,'rev-parse','HEAD'],{encoding:'utf8',stdio:['ignore','pipe','pipe']}).trim();}catch{catalog.push({product,state:'no-source-commit'});continue;}
  const dirty=execFileSync('git',['-C',root,'status','--porcelain','--untracked-files=no'],{encoding:'utf8'}).trim();
  const commands=new Map(),panels=[],menus=[];
  const crateRoot=path.join(root,'crates');
  for(const file of walk(crateRoot)){
    const relative=path.relative(root,file);
    if(!/\/(?:engine|ui-egui|tools|app)\/src\//.test('/'+relative))continue;
    const data=fs.readFileSync(file),text=data.toString('utf8');
    if(/\/panels\/[^/]+\.rs$/.test(relative)&&!relative.endsWith('/mod.rs'))panels.push({id:path.basename(file,'.rs'),source:relative,sha256:sha(data),status:'unmapped'});
    const commandPattern=/\bcmd!\s*\(\s*(?:query\s+)?"([^"\n]+)"\s*,\s*(?:tl!\s*\(\s*)?"([^"\n]+)"/g;
    for(const match of text.matchAll(commandPattern)){
      const [_,id,label]=match;
      const line=text.slice(0,match.index).split('\n').length;
      const prior=commands.get(id);
      if(prior)prior.sources.push({file:relative,line,sha256:sha(data)});
      else commands.set(id,{id,label,sources:[{file:relative,line,sha256:sha(data)}],status:'unmapped'});
    }
    if(/(?:^|\/)(?:catalog|menu_catalog)\.rs$/.test(relative)){
      const patterns=[
        /\bm!\s*\(\s*"([^"\n]+)"\s*,\s*"([^"\n]+)"\s*,\s*"([^"\n]+)"/g,
        /\(\s*&\[([^\]]+)\]\s*,\s*"([^"\n]+)"\s*,\s*(?:None|Some\([^)]*\))\s*,\s*"([^"\n]+)"/g,
      ];
      for(const pattern of patterns)for(const match of text.matchAll(pattern)){
        const [_,menu,label,id]=match;
        if(!id||id==='---')continue;
        menus.push({id,label,menu:menu.replaceAll('"',''),source:relative,line:text.slice(0,match.index).split('\n').length,sha256:sha(data),status:'unmapped-menu-declaration'});
      }
    }
  }
  catalog.push({product,commit,tracked_source_clean:!dirty,state:panels.length||commands.size||menus.length?'source-inventoried':'no-matching-editor-catalog',commands:[...commands.values()].sort((a,b)=>a.id.localeCompare(b.id)),panels:panels.sort((a,b)=>a.id.localeCompare(b.id)),menus});
}
const data={schema:1,boundary:'Source inventory only. A pane name or command declaration is not an implemented or qualified Omadesign feature. Dynamic commands, tools and file workflows also require direct inspection.',products:catalog};
fs.mkdirSync(path.dirname(path.resolve(output)),{recursive:true});
fs.writeFileSync(output,JSON.stringify(data,null,2)+'\n');
for(const product of catalog)process.stdout.write(`${product.product}: ${product.state}; ${product.commands?.length??0} command declarations, ${product.panels?.length??0} pane sources, ${product.menus?.length??0} catalog menu entries\n`);
