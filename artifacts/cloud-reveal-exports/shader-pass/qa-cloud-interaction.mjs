import {chromium} from '/tmp/omadesign-composition-qa/node_modules/playwright-core/index.mjs';
import fs from 'node:fs';
const b=await chromium.launch({executablePath:'/usr/bin/chromium',headless:true,args:['--no-sandbox','--use-angle=gles','--ozone-platform=x11'],env:{...process.env,DISPLAY:':0'}});
const p=await b.newPage();await p.goto('http://localhost:5174/cloud-dreamscape.html');
const result=await p.evaluate(async()=>{
 const {CloudRenderer}=await import('/src/components/cloud-reveal/renderer.ts');const c=document.createElement('canvas');const r=new CloudRenderer(c);r.resize(512,320,1);const gl=c.getContext('webgl');
 const state={time:30,entrance:10,pointerX:0,pointerY:0,pointerActive:0,scroll:0,reducedMotion:false,wakes:new Float32Array(32)};
 function pixels(){r.render(state);const a=new Uint8Array(c.width*c.height*4);gl.readPixels(0,0,c.width,c.height,gl.RGBA,gl.UNSIGNED_BYTE,a);return a}
 const baseline=pixels();state.wakes.set([.1,-.65,30,1.6]);const wake=pixels();state.wakes[2]=10;const faded=pixels();
 function delta(a,b){let sum=0,count=0;for(let i=0;i<a.length;i++){const d=Math.abs(a[i]-b[i]);sum+=d;if(d>3)count++}return {mean:sum/a.length,channelsChanged:count}}
 const result={gesture:delta(baseline,wake),dissipated:delta(baseline,faded),glError:gl.getError()};r.dispose();return result;
});console.log(result);fs.writeFileSync('/home/michael/Projects/omadesign/artifacts/cloud-reveal-exports/shader-pass/interaction.json',JSON.stringify(result,null,2));await b.close();if(result.gesture.channelsChanged<100 || result.dissipated.mean>.1 || result.glError)process.exit(1);
