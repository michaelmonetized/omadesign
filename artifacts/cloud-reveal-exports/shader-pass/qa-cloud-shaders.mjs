import {chromium} from '/tmp/omadesign-composition-qa/node_modules/playwright-core/index.mjs';
import fs from 'node:fs';
const out='/home/michael/Projects/omadesign/artifacts/cloud-reveal-exports/shader-pass';
const browser=await chromium.launch({executablePath:'/usr/bin/chromium',headless:true,args:['--no-sandbox','--use-angle=gles','--ozone-platform=x11'],env:{...process.env,DISPLAY:':0'}});
const report={errors:[],results:[]};
for(const spec of [{name:'desktop',width:1440,height:900},{name:'portrait',width:390,height:844,deviceScaleFactor:3,hasTouch:true},{name:'reduced',width:390,height:844,reducedMotion:'reduce'}]){
 const ctx=await browser.newContext({viewport:{width:spec.width,height:spec.height},deviceScaleFactor:spec.deviceScaleFactor||1,hasTouch:spec.hasTouch||false,reducedMotion:spec.reducedMotion||'no-preference'});
 const p=await ctx.newPage();p.on('pageerror',e=>report.errors.push(e.message));p.on('console',m=>{if(m.type()==='error')report.errors.push(m.text())});
 await p.goto('http://localhost:5186/cloud-dreamscape.html');
 await p.waitForFunction(()=>document.querySelector('.cloud-reveal')?.dataset.ambientTime>26 || document.querySelector('.cloud-reveal')?.dataset.motion==='paused',null,{timeout:45000});
 const data=await p.evaluate(async()=>{
 const root=document.querySelector('.cloud-reveal'),c=root.querySelector('canvas'),gl=c.getContext('webgl');
 const frames=[];await new Promise(resolve=>{let last=0;function tick(t){if(last)frames.push(t-last);last=t;if(frames.length<180)requestAnimationFrame(tick);else resolve()}requestAnimationFrame(tick)});
 return {dataset:{...root.dataset},glError:gl.getError(),overflow:document.documentElement.scrollWidth>innerWidth,buffer:[c.width,c.height],fps:1000/(frames.reduce((a,b)=>a+b)/frames.length),p95:frames.sort((a,b)=>a-b)[171]};
 });
 await p.screenshot({path:`${out}/${spec.name}.png`});
 if(spec.name==='desktop'){
  for(let i=0;i<30;i++)await p.mouse.move(300+i*26,720+Math.sin(i*.2)*50);
  await p.mouse.move(950,850);await p.mouse.down();await p.mouse.up();await p.waitForTimeout(150);await p.screenshot({path:`${out}/pointer-wake.png`});
  await p.evaluate(()=>scrollTo(0,300));await p.waitForTimeout(900);await p.screenshot({path:`${out}/scroll.png`});
  await p.evaluate(()=>scrollTo(0,2000));await p.waitForTimeout(400);
  const before=await p.locator('.cloud-reveal').getAttribute('data-ambient-time');await p.waitForTimeout(500);
  data.offscreenPaused=before===await p.locator('.cloud-reveal').getAttribute('data-ambient-time');
 }else if(spec.name==='portrait'){
  await p.touchscreen.tap(180,680);await p.waitForTimeout(150);await p.screenshot({path:`${out}/touch-wake.png`});
 }else {
  const before=await p.locator('.cloud-reveal').getAttribute('data-ambient-time');await p.waitForTimeout(500);data.static=before===await p.locator('.cloud-reveal').getAttribute('data-ambient-time');
 }
 report.results.push({name:spec.name,...data});console.log(JSON.stringify(report.results.at(-1)));await ctx.close();
}
await browser.close();fs.writeFileSync(out+'/qa.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report.errors));
