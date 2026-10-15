(()=>{
'use strict';
const stage=document.getElementById('stage'), overlay=document.getElementById('overlay');
const history=[], overlays=[], committed=new Map(), hoverRestore=new Map();
let current=__ROOT__, animation;
const template=id=>document.getElementById('screen-'+id);
const frame=()=>stage.firstElementChild;
const top=()=>overlays.length?overlays[overlays.length-1].node:frame();
const focus=node=>(node.querySelector('.interactive')||node).focus({preventScroll:true});
function restoreHover(){
 for(const [node,original] of hoverRestore){if(node.isConnected)node.replaceWith(original);}
 hoverRestore.clear();
}
function replaceVariant(node,id){
 const source=document.getElementById('variant-'+node.dataset.component+'-'+id);
 if(!source)throw new Error('The component variant is missing from this export');
 const next=source.content.firstElementChild.cloneNode(true);
 // A containing layout owns placement, while the variant owns its appearance.
 for(const key of ['position','left','right','top','bottom','flex','alignSelf','gridColumn','gridRow'])next.style[key]=node.style[key];
 next.dataset.variant=String(id);
 node.replaceWith(next);
 return next;
}
function applyCommitted(root){
 for(let pass=0;pass<64;pass++){
  let changed=false;
  for(const node of [root,...root.querySelectorAll('[data-component]')]){
   if(!node.isConnected||!committed.has(node.dataset.component))continue;
   const id=committed.get(node.dataset.component);
   if(node.dataset.variant===String(id))continue;
   const next=replaceVariant(node,id);if(node===root)root=next;changed=true;
  }
  if(!changed)return root;
 }
 throw new Error('Circular component variant state');
}
function mount(id,target){
 const source=template(id);if(!source)throw new Error('The destination frame is missing from this export');
 target.replaceChildren(source.content.cloneNode(true));
 return applyCommitted(target.firstElementChild);
}
function geometry(){
 if(!frame())return;
 const r=frame().getBoundingClientRect();
 Object.assign(overlay.style,{left:r.left+'px',top:r.top+'px',width:r.width+'px',height:r.height+'px'});
 for(const entry of overlays){
  const root=entry.node.firstElementChild;
  root.style.left=Math.max(0,(r.width-root.getBoundingClientRect().width)/2)+'px';
  root.style.top=Math.max(0,(r.height-root.getBoundingClientRect().height)/2)+'px';
 }
}
function modal(){
 frame().inert=overlays.length>0;
 overlays.forEach((entry,i)=>{entry.node.inert=i!==overlays.length-1;});
 if(overlays.length&&!overlay.open)overlay.showModal();
 if(!overlays.length&&overlay.open)overlay.close();
 geometry();
}
function closeOverlay(){
 const entry=overlays.pop();if(!entry)return false;
 entry.node.remove();modal();
 if(entry.focus&&entry.focus.isConnected&&!entry.focus.closest('[inert]'))entry.focus.focus({preventScroll:true});
 else focus(top());
 return true;
}
function snapshot(){
 const r=frame().getBoundingClientRect(), canvas=document.createElement('div');
 Object.assign(canvas.style,{position:'fixed',left:r.left+'px',top:r.top+'px',width:r.width+'px',height:r.height+'px',overflow:'hidden',pointerEvents:'none',zIndex:'2147483647'});
 const base=frame().cloneNode(true);base.inert=true;canvas.append(base);
 for(const entry of overlays)canvas.append(entry.node.cloneNode(true));
 canvas.querySelectorAll('[id]').forEach(n=>n.removeAttribute('id'));
 return canvas;
}
function transition(before,interaction){
 if(animation){animation();animation=null;}
 const duration=Math.min(10000,Math.max(0,Number(interaction.duration_ms)||0));
 if(interaction.transition==='Instant'||!duration||matchMedia('(prefers-reduced-motion: reduce)').matches)return;
 const after=snapshot(), old=document.createElement('div'), next=document.createElement('div');
 Object.assign(old.style,{position:'absolute',inset:'0'});Object.assign(next.style,{position:'absolute',inset:'0'});
 old.append(...before.childNodes);next.append(...after.childNodes);before.append(old,next);
 // Native Present uses smoothstep on the complete composited canvas.
 const keys=Array.from({length:61},(_,i)=>{const t=i/60,s=t*t*(3-2*t),d=interaction.transition==='SlideLeft'?-1:1;
  return interaction.transition==='Dissolve'?{offset:t,opacity:s}:{offset:t,transform:'translateX('+(-d*(1-s)*100)+'%)'};});
 const oldKeys=keys.map((k,i)=>interaction.transition==='Dissolve'?{offset:k.offset,opacity:1}:{offset:k.offset,transform:'translateX('+((interaction.transition==='SlideLeft'?-1:1)*(i/60)**2*(3-2*i/60)*100)+'%)'});
 // A dialog is in the top layer; its animation proxy must live there as well.
 (overlay.open?overlay:document.body).append(before);
 if(overlay.open){const r=overlay.getBoundingClientRect();before.style.left=r.left+'px';before.style.top=r.top+'px';}
 const animations=[old.animate(oldKeys,{duration,easing:'linear',fill:'both'}),next.animate(keys,{duration,easing:'linear',fill:'both'})];
 const cleanup=()=>{animations.forEach(a=>a.cancel());before.remove();};animation=cleanup;
 Promise.all(animations.map(a=>a.finished.catch(()=>{}))).then(()=>{cleanup();if(animation===cleanup)animation=null;});
}
function validate(node,handlers){
 for(const i of handlers){const a=i.action;
  if((a.Navigate&&!template(a.Navigate.target))||(a.OpenOverlay&&!template(a.OpenOverlay.target)))throw new Error('The destination frame is missing from this export');
  if(a.SetVariant){const instance=node.closest('[data-component]');if(!instance||!document.getElementById('variant-'+instance.dataset.component+'-'+a.SetVariant.component))throw new Error('The component variant is missing from this export');}
 }
}
function act(node,i){
 const a=i.action;
 if(a.Navigate){
  if(current===a.Navigate.target)return false;
  restoreHover();history.push(current);current=a.Navigate.target;
  for(const entry of overlays)entry.node.remove();overlays.length=0;
  mount(current,stage);modal();focus(frame());return true;
 }
 if(a==='Back'){
  if(closeOverlay())return true;
  if(!history.length)return false;
  restoreHover();current=history.pop();mount(current,stage);focus(frame());geometry();return true;
 }
 if(a.OpenOverlay){
  const id=a.OpenOverlay.target;if(current===id||overlays.some(e=>e.id===id))return false;
  const layer=document.createElement('div');layer.className='overlay-layer';layer.dataset.overlay=String(id);
  overlay.append(layer);mount(id,layer);overlays.push({id,node:layer,focus:document.activeElement});modal();focus(layer);return true;
 }
 if(a==='CloseOverlay')return closeOverlay();
 if(a.SetVariant){
  const instance=node.closest('[data-component]'),key=instance.dataset.component,id=a.SetVariant.component;
  if(i.trigger==='Hover'){
   if(!hoverRestore.has(instance))hoverRestore.set(instance,instance.cloneNode(true));
   const original=hoverRestore.get(instance);hoverRestore.delete(instance);
   const next=replaceVariant(instance,id);hoverRestore.set(next,original);applyCommittedChildren(next);return true;
  }
  committed.set(key,id);
  for(const [hovered] of hoverRestore)if(hovered.dataset.component===key)hoverRestore.delete(hovered);
  const next=replaceVariant(instance,id);applyCommittedChildren(next);geometry();return true;
 }
 return false;
}
function applyCommittedChildren(node){for(const child of node.querySelectorAll('[data-component]'))if(child.isConnected)applyCommitted(child);}
function dispatch(event,trigger){
 const node=event.target.closest('[data-node]');if(!node||!top().contains(node)||node.closest('[inert]'))return;
 for(let ancestor=node;ancestor&&ancestor!==document.body;ancestor=ancestor.parentElement){
  const handlers=JSON.parse(ancestor.dataset.actions||'[]').filter(i=>i.trigger===trigger);
  if(!handlers.length)continue;
  if(trigger==='Hover'&&event.relatedTarget&&ancestor.contains(event.relatedTarget))return;
  event.preventDefault();
  try{validate(ancestor,handlers);const before=snapshot();let changed=false;for(const i of handlers)changed=act(ancestor,i)||changed;if(changed)transition(before,handlers[handlers.length-1]);}
  catch(error){document.getElementById('prototype-error').textContent=error.message;}
  return;
 }
}
document.addEventListener('click',e=>dispatch(e,'Click'));
document.addEventListener('pointerover',e=>dispatch(e,'Hover'));
document.addEventListener('pointerdown',e=>dispatch(e,'Press'));
document.addEventListener('pointerout',event=>{
 for(const [node,original] of hoverRestore)if(node.contains(event.target)&&(!event.relatedTarget||!node.contains(event.relatedTarget))){node.replaceWith(original);hoverRestore.delete(node);geometry();}
});
document.addEventListener('keydown',e=>{if((e.key==='Enter'||e.key===' ')&&e.target.matches('.interactive'))dispatch(e,'Click');});
overlay.addEventListener('cancel',e=>{e.preventDefault();closeOverlay();});
overlay.addEventListener('click',e=>{if(e.target===overlay||e.target===overlays.at(-1)?.node)closeOverlay();});
new ResizeObserver(geometry).observe(stage);
window.addEventListener('resize',geometry);
mount(current,stage);
})();
