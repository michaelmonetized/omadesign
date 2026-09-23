import { forwardRef, memo, useEffect, useId, useImperativeHandle, useRef } from "react";
import { cloudBrand } from "./brand";
import { cloudLetters } from "./letters";
import { CloudRenderer, type DreamFrame } from "./renderer";
import "./reveal.css";

/** Entrance is a one-time gesture; the atmosphere has an independent live clock. */
export const CLOUD_ENTRANCE_DURATION = 9.2;
export interface CloudRevealHandle {
  setScroll(progress: number): void;
  setMotion(enabled: boolean): void;
  finishEntrance(): void;
}
interface Props {
  onSettled?: () => void;
  onContentProgress?: (visibleCount: number, ready: boolean) => void;
  onMotionChange?: (enabled: boolean) => void;
}
const clamp = (value: number) => Math.max(0, Math.min(1, value));
const smooth = (a: number, b: number, value: number) => {
  const t=clamp((value-a)/(b-a)); return t*t*(3-2*t);
};
const easeOut = (a: number,b: number,value: number) => 1-Math.pow(1-clamp((value-a)/(b-a)),3);

// Static artwork is never reconciled on animation frames or homepage state changes.
const BrandArtwork=memo(function BrandArtwork({id}:{id:string}) {
  return <div className="dream-lockup" aria-hidden="true">
    <svg className="dream-icon" viewBox={cloudBrand.icon.viewBox}>
      <defs><mask id={`${id}-ring`} maskUnits="userSpaceOnUse" x="-1070" y="440" width="1040" height="1040">
        <circle data-ring-draw cx="-552" cy="960" r="464" fill="none" stroke="white" strokeWidth="128" pathLength="1" strokeDasharray="1" strokeDashoffset="1" transform="rotate(-90 -552 960)" />
      </mask></defs>
      <g mask={`url(#${id}-ring)`}>{cloudBrand.icon.paths.map((path,index)=><path key={index} d={path.d} fillRule={path.fillRule} fill="currentColor" />)}</g>
    </svg>
    <svg className="dream-wordmark" viewBox={cloudBrand.wordmark.viewBox}>
      <defs>{cloudLetters.map((letter,index)=><mask key={index} id={`${id}-letter-${index}`} maskUnits="userSpaceOnUse" x={letter.bounds.x-24} y={letter.bounds.y-24} width={letter.bounds.width+48} height={letter.bounds.height+48}>
        <path data-letter-draw d={letter.d} fill="none" stroke="white" strokeWidth="38" strokeLinejoin="round" strokeLinecap="round" pathLength="1" strokeDasharray="1" strokeDashoffset="1" />
      </mask>)}</defs>
      <g data-letter-sequence>{cloudLetters.map((letter,index)=><path key={index} d={letter.d} fillRule={letter.fillRule} fill="white" mask={`url(#${id}-letter-${index})`} />)}</g>
      <g data-wordmark-final opacity="0">{cloudBrand.wordmark.paths.map((path,index)=><path key={index} d={path.d} fillRule={path.fillRule} fill="white" />)}</g>
    </svg>
  </div>;
});

export const CloudReveal=forwardRef<CloudRevealHandle,Props>(function CloudReveal(props,ref) {
  const root=useRef<HTMLDivElement>(null),canvas=useRef<HTMLCanvasElement>(null);
  const callbacks=useRef(props); callbacks.current=props;
  const api=useRef<CloudRevealHandle | null>(null);
  const id=`dream-${useId().replace(/[^a-zA-Z0-9]/g,"")}`;
  useImperativeHandle(ref,()=>({
    setScroll:value=>api.current?.setScroll(value),
    setMotion:enabled=>api.current?.setMotion(enabled),
    finishEntrance:()=>api.current?.finishEntrance(),
  }),[]);

  useEffect(()=>{
    const element=root.current,surface=canvas.current;
    if(!element || !surface) return;
    const host=element.closest<HTMLElement>("[data-cloud-experience]") || element;
    const lockup=element.querySelector<HTMLElement>(".dream-lockup")!;
    const ring=element.querySelector<SVGCircleElement>("[data-ring-draw]")!;
    const icon=element.querySelector<SVGSVGElement>(".dream-icon")!;
    const letters=Array.from(element.querySelectorAll<SVGPathElement>("[data-letter-draw]"));
    const sequence=element.querySelector<SVGGElement>("[data-letter-sequence]")!;
    const finalWordmark=element.querySelector<SVGGElement>("[data-wordmark-final]")!;
    const preference=matchMedia("(prefers-reduced-motion: reduce)");
    let renderer:CloudRenderer | null=null,disposed=false,frame=0,previous=0;
    let motion=!preference.matches,inView=false,notified=false,brandFinished=false;
    let width=1,height=1,left=0,top=0,settledScale=.68,settledY=.30,settledOpacity=1;
    let targetX=0,targetY=0,targetActive=0,targetScroll=0;
    let lastBrandEntrance=-1,lastBrandScroll=-1,diagnosticCounter=0;
    let contentTime=0,lastContentStage=-1;
    const drawContent=()=>{
      const count=Math.min(5,Math.max(0,Math.floor((contentTime-3)/2.7)+1));
      const ready=contentTime>=24.541667;
      const stage=ready?6:count;
      if(stage!==lastContentStage){lastContentStage=stage;callbacks.current.onContentProgress?.(count,ready);}
    };
    const state:DreamFrame={time:0,entrance:0,pointerX:0,pointerY:0,pointerActive:0,scroll:0,reducedMotion:!motion};
    const initialize=()=>{
      try { renderer=new CloudRenderer(surface); renderer.resize(width,height,devicePixelRatio||1); element.dataset.renderer="webgl"; }
      catch { renderer=null; element.dataset.renderer="fallback"; }
    };
    const announce=()=>{
      if(notified) return;
      notified=true; element.dataset.settled="true"; callbacks.current.onSettled?.();
    };
    const drawBrand=()=>{
      const t=state.entrance,exit=state.scroll;
      if(t===lastBrandEntrance && Math.abs(exit-lastBrandScroll)<.0001) return;
      lastBrandEntrance=t; lastBrandScroll=exit;
      const settle=easeOut(7.45,CLOUD_ENTRANCE_DURATION,t);
      const scale=1-(1-settledScale)*settle;
      lockup.style.transform=`translate3d(0,${-height*((.40-settledY)*settle+.18*exit)}px,0) scale(${scale*(1-.07*exit)})`;
      lockup.style.opacity=String((1-(1-settledOpacity)*settle)*(1-smooth(.50,.96,exit)));
      element.style.setProperty("--dream-rise",String(smooth(0,3.5,t)));
      element.style.setProperty("--dream-exit",String(exit));
      if(!brandFinished) {
        icon.style.opacity=String(smooth(2.96,3.04,t));
        ring.style.strokeDashoffset=String(1-smooth(3.0,4.65,t));
        letters.forEach((letter,index)=>{
          const start=4.65+index*.27;
          letter.style.strokeDashoffset=String(1-easeOut(start,start+.42,t));
        });
        const filled=smooth(7.20,7.35,t);
        sequence.style.opacity=String(1-filled); finalWordmark.style.opacity=String(filled);
        if(t>=CLOUD_ENTRANCE_DURATION) {
          brandFinished=true; sequence.style.display="none";
          // Remove the completed SVG mask from the settled logo's paint path.
          element.querySelector(".dream-icon > g")?.removeAttribute("mask");
          announce();
        }
      }
    };
    const draw=()=>{
      renderer?.render(state); drawBrand(); drawContent();
      // Low-frequency diagnostics for browser verification; no React frame state.
      if((diagnosticCounter++%20)===0 || !motion) {
        element.dataset.ambientTime=state.time.toFixed(3);
        element.dataset.entrance=state.entrance.toFixed(3);
        element.dataset.scroll=state.scroll.toFixed(3);
        element.dataset.pointer=`${state.pointerX.toFixed(2)},${state.pointerY.toFixed(2)}`;
      }
    };
    const tick=(now:number)=>{
      frame=0;
      if(disposed || !inView || document.hidden || !motion) { previous=0; return; }
      const dt=previous ? Math.min(.06,(now-previous)/1000) : 0;
      previous=now;
      state.time+=dt; contentTime+=dt; state.entrance=Math.min(CLOUD_ENTRANCE_DURATION,state.entrance+dt);
      const ease=1-Math.exp(-dt*6),scrollEase=1-Math.exp(-dt*14);
      state.pointerX+=(targetX-state.pointerX)*ease;
      state.pointerY+=(targetY-state.pointerY)*ease;
      state.pointerActive+=(targetActive-state.pointerActive)*ease;
      state.scroll+=(targetScroll-state.scroll)*scrollEase;
      renderer?.sampleFrame(now); draw(); frame=requestAnimationFrame(tick);
    };
    const schedule=()=>{
      const active=inView && !document.hidden && motion;
      element.dataset.motion=active ? "running" : "paused";
      if(active && !frame) { previous=0; frame=requestAnimationFrame(tick); }
      if(!active) { cancelAnimationFrame(frame); frame=0; previous=0; }
    };
    const finishEntrance=()=>{ contentTime=24.541667; state.entrance=CLOUD_ENTRANCE_DURATION; drawBrand(); draw(); announce(); };
    const setMotion=(enabled:boolean)=>{
      motion=enabled; state.reducedMotion=!enabled;
      if(!enabled) { state.pointerX=0;state.pointerY=0;state.pointerActive=0;state.scroll=targetScroll;finishEntrance(); }
      callbacks.current.onMotionChange?.(motion); schedule();
    };
    api.current={
      setScroll:progress=>{targetScroll=Number.isFinite(progress)?clamp(progress):0; if(!motion) {state.scroll=targetScroll;draw();}},
      setMotion,finishEntrance,
    };
    const resize=()=>{
      const rect=element.getBoundingClientRect(); width=rect.width; height=rect.height;left=rect.left;top=rect.top;
      const markSize=Math.min(height*.32,width*.56,420);
      settledScale=.68;settledY=.30;settledOpacity=1;
      if(width<=800){
        // Fit the artwork into the existing layout's available air; never move
        // the original content to make room for a decorative background.
        const copy=host.querySelector<HTMLElement>(".cloud-copy")?.getBoundingClientRect();
        const header=host.querySelector<HTMLElement>(".cloud-topline")?.getBoundingClientRect();
        const upper=header ? header.bottom-top : 60;
        const gap=copy ? copy.top-top-upper : 0;
        if(gap>58){settledScale=Math.min(84,gap-28,width*.26)/markSize;settledY=(upper+gap*.5)/height;}
        else {settledScale=width*.23/markSize;settledY=.35;settledOpacity=.28;}
      }
      element.style.setProperty("--dream-mark-size",`${markSize}px`);
      renderer?.resize(width,height,devicePixelRatio||1);
      lastBrandEntrance=-1;draw();
    };
    const pointer=(event:PointerEvent)=>{
      if(!motion || event.pointerType==="touch") return;
      targetX=clamp((event.clientX-left)/Math.max(1,width))*2-1;
      targetY=1-clamp((event.clientY-top)/Math.max(1,height))*2;
      targetActive=1;
    };
    const leave=()=>{targetActive=0;targetX=0;targetY=0;};
    const changePreference=()=>setMotion(!preference.matches);
    const lost=(event:Event)=>{event.preventDefault();renderer=null;element.dataset.renderer="fallback";};
    const restored=()=>{if(!disposed){initialize();resize();}};
    resize(); initialize(); draw();
    const observer=new IntersectionObserver(([entry])=>{inView=entry.isIntersecting;schedule();},{threshold:0});
    observer.observe(element);
    const sizeObserver=new ResizeObserver(resize);sizeObserver.observe(element);
    const copy=host.querySelector(".cloud-copy");if(copy)sizeObserver.observe(copy);
    host.addEventListener("pointermove",pointer,{passive:true}); host.addEventListener("pointerleave",leave);
    surface.addEventListener("webglcontextlost",lost);surface.addEventListener("webglcontextrestored",restored);
    document.addEventListener("visibilitychange",schedule);preference.addEventListener("change",changePreference);
    callbacks.current.onMotionChange?.(motion);
    if(!motion) finishEntrance();
    return ()=>{
      disposed=true;api.current=null;cancelAnimationFrame(frame);observer.disconnect();sizeObserver.disconnect();
      host.removeEventListener("pointermove",pointer);host.removeEventListener("pointerleave",leave);
      document.removeEventListener("visibilitychange",schedule);preference.removeEventListener("change",changePreference);
      surface.removeEventListener("webglcontextlost",lost);surface.removeEventListener("webglcontextrestored",restored);
      renderer?.dispose();
    };
  },[]);

  return <div ref={root} className="cloud-reveal" data-settled="false" role="img" aria-label="Omadesign’s lime O maze and white wordmark above a living sea of soft moonlit clouds and golden fireflies.">
    <div className="dream-fallback" aria-hidden="true">
      <div className="dream-fallback-clouds">{Array.from({length:9},(_,i)=><i key={i} style={{left:`${i*15-16}%`,bottom:`${-6+(i%3)*5}%`,width:`${25+(i%3)*7}%`,animationDelay:`${-i*2}s`}} />)}</div>
      <div className="dream-fallback-fireflies">{Array.from({length:18},(_,i)=><i key={i} style={{left:`${7+(i*47)%87}%`,top:`${18+(i*23)%63}%`,animationDelay:`${-i*.7}s`}} />)}</div>
    </div>
    <canvas ref={canvas} aria-hidden="true" />
    <BrandArtwork id={id} />
  </div>;
});
