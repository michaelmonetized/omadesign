import {chromium} from '/tmp/omadesign-composition-qa/node_modules/playwright-core/index.mjs';
import fs from 'node:fs';
const b=await chromium.launch({executablePath:'/usr/bin/chromium',headless:true,args:['--no-sandbox','--use-angle=gles','--ozone-platform=x11'],env:{...process.env,DISPLAY:':0'}});
const c=await b.newContext({viewport:{width:390,height:844},hasTouch:true,isMobile:true});const p=await c.newPage();await p.goto('http://localhost:5186/cloud-dreamscape.html');await p.waitForTimeout(1200);
const client=await c.newCDPSession(p);
await client.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:180,y:700}]});
for(let y=680;y>=300;y-=20){await client.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:180,y}]});await p.waitForTimeout(20)}
await client.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await p.waitForTimeout(300);
const scroll=await p.evaluate(()=>scrollY);await p.evaluate(()=>scrollTo(0,0));await p.emulateMedia({reducedMotion:'reduce'});await p.waitForTimeout(300);const first=await p.locator('.cloud-reveal').getAttribute('data-ambient-time');await p.waitForTimeout(300);const second=await p.locator('.cloud-reveal').getAttribute('data-ambient-time');
const result={nativeTouchScroll:scroll>0,scrollY:scroll,liveReducedMotionStops:first===second};console.log(result);fs.writeFileSync('/home/michael/Projects/omadesign/artifacts/cloud-reveal-exports/shader-pass/touch-scroll.json',JSON.stringify(result,null,2));await b.close();if(!result.nativeTouchScroll||!result.liveReducedMotionStops)process.exit(1);
