/** Deterministic export of the same SVG/WebGL composition that plays on the site.
 * PLAYWRIGHT_MODULE=/path/to/playwright-core/index.mjs node scripts/render-cloud-reveal.mjs
 * --url http://localhost:5175/cloud-film.html --out /tmp/cloud-reveal.mp4
 * --width 3840 --height 2160 --fps 30
 */
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { once } from "node:events";

const args=Object.fromEntries(Array.from({length:Math.ceil((process.argv.length-2)/2)},(_,i)=>[process.argv[2+i*2].replace(/^--/,""),process.argv[3+i*2]]));
const width=Number(args.width || 1920), height=Number(args.height || 1080), fps=Number(args.fps || 30);
if(!Number.isInteger(width)||!Number.isInteger(height)||width<64||height<64||width%2||height%2||!Number.isFinite(fps)||fps<1||fps>60) throw new Error("Use positive even dimensions and 1–60 fps.");
const out=resolve(args.out || "artifacts/cloud-reveal-exports/omadesign-cloud-1080p.mp4");
const url=new URL(args.url || "http://localhost:5175/cloud-film.html");
url.searchParams.set("clean","1"); url.searchParams.set("t","0"); url.searchParams.set("quality","full");
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || "playwright-core");
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH || "/usr/bin/chromium",headless:true,
  env:{...process.env,DISPLAY:process.env.DISPLAY || ":0"},
  args:["--no-sandbox","--ozone-platform=x11","--use-gl=angle","--use-angle=gles","--enable-webgl","--ignore-gpu-blocklist"]});
let encoder;
try {
  await mkdir(dirname(out),{recursive:true});
  const context=await browser.newContext({viewport:{width,height},deviceScaleFactor:1});
  const page=await context.newPage();
  const failures=[]; page.on("pageerror",e=>failures.push(String(e)));
  await page.goto(url.href,{waitUntil:"networkidle"});
  await page.waitForFunction(()=>window.omadesignCloud && document.querySelector('.cloud-reveal')?.dataset.renderer==="webgl");
  encoder=spawn("ffmpeg",["-hide_banner","-loglevel","warning","-n","-f","image2pipe","-framerate",String(fps),"-vcodec","mjpeg","-i","pipe:0","-i",resolve("site/public/media/cloud/reveal-score.m4a"),"-c:v","libx264","-preset","medium","-crf","18","-pix_fmt","yuv420p","-c:a","copy","-movflags","+faststart","-t","20",out],{stdio:["pipe","inherit","inherit"]});
  const completed=once(encoder,"close");
  let encoderFailure;
  encoder.stdin.on("error",error=>{encoderFailure=error;});
  for(let frame=0;frame<20*fps;frame++) {
    if(encoderFailure) throw encoderFailure;
    await page.evaluate(t=>window.omadesignCloud.seek(t),frame/fps);
    // High-quality JPEG keeps frame capture bounded; review stills below are PNG.
    const jpeg=await page.screenshot({type:"jpeg",quality:98});
    if(!encoder.stdin.write(jpeg)) await once(encoder.stdin,"drain");
    if(frame%fps===0) process.stdout.write(`Rendered ${frame/fps}/20 seconds\n`);
    if([0,5,8,12,16,19].includes(frame/fps)) await page.screenshot({path:`${dirname(out)}/frame-${String(frame/fps).padStart(2,"0")}.png`,type:"png"});
  }
  encoder.stdin.end();
  const [code]=await completed;
  if(code!==0) throw new Error(`ffmpeg exited ${code}`);
  if(failures.length) throw new Error(failures.join("\n"));
  await writeFile(`${out}.json`,JSON.stringify({url:url.href,width,height,fps,duration:20,renderedAt:new Date().toISOString(),errors:failures},null,2));
  process.stdout.write(`Exported ${out}\n`);
} finally {
  if(encoder && encoder.exitCode===null) encoder.kill("SIGTERM");
  await browser.close();
}
