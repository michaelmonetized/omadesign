import moonMapUrl from "./textures/lroc-color-2k.jpg";

/** The atmosphere is a live scene. Expensive cloud lighting is baked once into
 * a small GPU atlas; each animation frame draws a few softly lit volume sprites.
 * The exact brand artwork is rendered separately as SVG. */
export type DreamFrame = {
  time: number;
  entrance: number;
  pointerX: number;
  pointerY: number;
  pointerActive: number;
  scroll: number;
  reducedMotion: boolean;
};

const screenVertex = `attribute vec2 aPosition;
varying vec2 vUv;
void main(){vUv=aPosition*.5+.5;gl_Position=vec4(aPosition,0.,1.);}`;

// One filtered lookup gives trilinear, periodic volume noise. Adjacent Z
// slices are packed into R/G, avoiding procedural hash work on live pixels.
const volumeNoise = `uniform sampler2D uNoise;
float noise3(vec3 p){
  vec3 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);
  vec2 uv=i.xy+vec2(37.,17.)*i.z+f.xy;
  vec2 rg=texture2D(uNoise,(uv+.5)/256.).rg;
  return mix(rg.x,rg.y,f.z);
}
float fbm(vec3 p){return noise3(p)*.54+noise3(p*2.03+19.7)*.28+noise3(p*4.11+31.3)*.12+noise3(p*8.23+47.1)*.06;}
`;

// A one-time volume integration gives each puff its internal shadows and
// multiple scales of billowing density. The live scene reuses this lighting.
const atlasFragment = `precision highp float;
varying vec2 vUv;
${volumeNoise}
float hash(float p){return fract(sin(p*127.1+311.7)*43758.5453);}
vec4 lobe(vec3 p,vec3 center,vec3 radius){
  vec3 q=(p-center)/radius;float d=max(0.,1.-dot(q,q));
  return vec4(d*d*14.4,-57.6*d*q/radius);
}
vec4 field(vec3 p,float seed){
  vec4 d=lobe(p,vec3(0.,-.17,0.),vec3(1.34,.31,.49));
  for(int i=0;i<13;i++){
    float k=float(i),r=.32+hash(seed+k*4.3)*.22;
    float x=-1.1+mod(k,5.)*.54;
    float y=-.01+(1.-abs(x)*.56)*.31+hash(k*9.7+seed)*.14;
    float z=(hash(k*3.8+seed)-.5)*.64;
    if(i>4){x=-1.1+(k-5.)*.31;y=-.16+hash(k+seed)*.40;z=.22+hash(k*2.3+seed)*.46;r*=.62+hash(k*6.+seed)*.20;}
    d+=lobe(p,vec3(x,y,z),vec3(r,r*(.86+hash(k+seed)*.2),r));
  }
  return d;
}
float detail(vec3 p,float seed){return fbm(p*6.3+vec3(seed*3.7,seed,seed*1.3));}
float density(vec3 p,float seed){
  float shape=sqrt(field(p,seed).x)*1.25;
  return max(0.,shape-(1.-detail(p,seed))*1.65-.12)*2.3;
}
void main(){
  vec2 tile=floor(vUv*vec2(4.,2.));float seed=tile.x+tile.y*4.+2.14;
  vec2 uv=fract(vUv*vec2(4.,2.));
  vec2 xy=vec2((uv.x-.5)*3.6,(uv.y-.5)*2.3+.13);
  vec4 sum=vec4(0.);vec3 light=normalize(vec3(-.45,.85,.70));
  for(int step=0;step<60;step++){
    vec3 p=vec3(xy,1.18-float(step)*.04);
    float d=density(p,seed);
    if(d>.006){
      vec4 base=field(p,seed);
      float n=detail(p,seed);
      vec3 rough=vec3(detail(p+vec3(.025,0.,0.),seed)-n,detail(p+vec3(0.,.025,0.),seed)-n,detail(p+vec3(0.,0.,.025),seed)-n);
      vec3 normal=normalize(-base.yzw*.055-rough*10.5+vec3(0.,0.,.001));
      float optical=density(p+light*.10,seed)*.10+density(p+light*.27,seed)*.20+density(p+light*.55,seed)*.32;
      float direct=exp(-optical*.72),multiple=exp(-optical*.15);
      float diffuse=.38+.62*max(0.,dot(normal,light));
      vec3 ambient=mix(vec3(.10,.092,.155),vec3(.23,.225,.32),normal.y*.5+.5);
      vec3 color=ambient+vec3(.83,.84,.91)*direct*diffuse+vec3(.15,.13,.22)*multiple;
      // Forward scattering brightens thin rims without flattening their cores.
      color+=vec3(.21,.21,.29)*pow(direct,2.)*(1.-max(0.,normal.z))*.7;
      float a=1.-exp(-d*.16);
      sum.rgb+=(1.-sum.a)*color*a;sum.a+=(1.-sum.a)*a;
    }
    if(sum.a>.996)break;
  }
  float border=smoothstep(0.,.02,min(min(uv.x,uv.y),min(1.-uv.x,1.-uv.y)));
  gl_FragColor=sum*border;
}`;

const skyFragment = `precision highp float;
varying vec2 vUv;
uniform vec2 uResolution;
uniform float uTime,uEntrance,uScroll,uMoonRadius,uMoonLoaded;
uniform vec2 uPointer;
uniform sampler2D uMoon;
${volumeNoise}
float hash(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
void main(){
  float aspect=uResolution.x/uResolution.y;
  vec2 p=(vUv-.5)*vec2(aspect,1.);
  vec2 gaze=p+uPointer*.003;
  vec3 sky=mix(vec3(.095,.057,.16),vec3(.013,.010,.037),smoothstep(0.,1.,vUv.y));
  // Broad galactic dust and finer filaments sit behind several star depths.
  vec3 nebula=vec3(gaze*vec2(1.7,2.6),2.4);
  float dust=fbm(nebula*2.2);
  float ribbon=exp(-pow((gaze.y-.10-gaze.x*.20+(dust-.5)*.4)*2.1,2.));
  float fine=fbm(nebula*6.3+12.);
  sky+=vec3(.085,.050,.145)*ribbon*smoothstep(.30,.76,dust);
  sky+=vec3(.027,.033,.064)*ribbon*smoothstep(.46,.72,fine);
  sky+=vec3(.030,.023,.050)*exp(-dot(p*vec2(.55,1.),p*vec2(.55,1.))*4.);
  for(int j=0;j<3;j++){
    float layer=float(j);float frequency=115.+layer*82.;
    vec2 grid=(p+uPointer*(.0015+layer*.001))*frequency;
    vec2 cell=floor(grid),f=fract(grid)-.5;float seed=hash(cell+layer*7.3);
    vec2 offset=vec2(hash(cell+1.2),hash(cell+8.9))-.5;
    vec2 delta=f-offset*.7;float r2=dot(delta,delta);
    float star=exp(-r2*(155.+layer*80.))*step(.970+layer*.003,seed);
    float halo=exp(-r2*17.)*step(.9975,seed)*.065;
    float pulse=.82+.18*sin(uTime*(.25+seed*.4)+seed*120.);
    sky+=mix(vec3(.42,.60,.90),vec3(.95,.86,.72),hash(cell+3.1))*(star*.80+halo)*pulse;
  }
  float rise=smoothstep(.8,2.8,uEntrance);
  float fade=smoothstep(1.25,2.5,uEntrance)*(1.-smoothstep(6.5,8.5,uEntrance))*(1.-smoothstep(0.,.5,uScroll));
  vec2 moon=(p-vec2(0.,mix(-.25,.10,rise)))/uMoonRadius;
  float r=length(moon);
  sky+=vec3(.21,.19,.32)*exp(-r*r*.20)*fade;
  sky+=vec3(.25,.25,.35)*exp(-r*r*.88)*fade;
  sky+=vec3(.13,.16,.23)*exp(-abs(r-1.)*8.)*fade;
  if(r<1.){
    float z=sqrt(1.-r*r);vec3 normal=vec3(moon,z);
    // LROC map is centered on the near side (zero longitude).
    vec2 uv=vec2(.5+atan(normal.x,normal.z)/6.2831853,.5-asin(normal.y)/3.14159265);
    vec3 albedo=texture2D(uMoon,uv).rgb;
    albedo=mix(vec3(.67+.22*fbm(normal*8.)),albedo,uMoonLoaded);
    float lighting=.23+.77*max(0.,dot(normal,normalize(vec3(-.38,.27,1.2))));
    vec3 color=pow(albedo,vec3(.83))*vec3(1.13,1.14,1.22)*lighting;
    sky=mix(sky,color,fade*(1.-smoothstep(.993,1.,r)));
  }
  sky=mix(sky,vec3(.23,.20,.32),.62*(1.-smoothstep(.3,2.8,uEntrance)));
  sky*=1.-.25*smoothstep(.3,1.2,length(p*vec2(.5,1.)));
  gl_FragColor=vec4(sky,1.);
}`;

const mistFragment = `precision highp float;
varying vec2 vUv;
uniform vec2 uResolution,uPointer;
uniform float uTime,uEntrance,uScroll,uLayer;
${volumeNoise}
void main(){
  float aspect=uResolution.x/uResolution.y;
  vec2 p=(vUv-.5)*vec2(aspect,1.);
  float rise=smoothstep(0.,4.8,uEntrance);
  float height=mix(.08,-.14,rise)-uLayer*.18+uScroll*.72;
  vec3 flow=vec3(p*vec2(2.8,11.5),uLayer*17.4+uTime*.035);
  flow.x-=uTime*(.10+uLayer*.065);flow.y+=uTime*.025;
  flow.xy+=uPointer*vec2(.10,.035);
  float warp=noise3(flow*.48+3.1);
  float n=fbm(flow+vec3(warp*1.4,warp*.9,0.));
  float curl=fbm(flow*1.8+vec3(n*2.,0.,11.));
  float ribbon=exp(-pow((p.y-height+(n-.5)*.24)*5.5,2.));
  float wisps=smoothstep(.28,.70,n)*.72+smoothstep(.48,.75,curl)*.28;
  float a=wisps*ribbon*(.38-uLayer*.10)*(1.-smoothstep(.62,1.,uScroll));
  vec3 color=mix(vec3(.28,.25,.40),vec3(.64,.63,.78),n);
  float light=exp(-dot(p-vec2(-.12,.15),p-vec2(-.12,.15))*2.1);
  color+=vec3(.10,.09,.15)*light;
  gl_FragColor=vec4(color*a,a);
}`;

const cloudVertex = `attribute vec2 aPosition;
varying vec2 vUv;
uniform vec4 uQuad;
uniform float uAspect;
uniform float uAngle;
void main(){
  vUv=aPosition*.5+.5;
  vec2 p=aPosition*uQuad.zw*.5;
  float c=cos(uAngle),s=sin(uAngle);
  p=mat2(c,-s,s,c)*p+uQuad.xy;
  gl_Position=vec4(p.x*2./uAspect,p.y*2.,0.,1.);
}`;
const cloudFragment = `precision highp float;
varying vec2 vUv;
uniform sampler2D uAtlas;
uniform vec2 uTile;
uniform vec4 uTint;
uniform float uTime,uPhase;
${volumeNoise}
void main(){
  vec3 flow=vec3(vUv*vec2(6.,5.),uPhase+uTime*.065);
  float drift=noise3(flow);
  vec2 warp=vec2(drift-.5,noise3(flow+8.4)-.5)*.018;
  vec2 uv=(uTile+clamp(vUv+warp,vec2(.002),vec2(.998)))/vec2(4.,2.);
  vec4 cloud=texture2D(uAtlas,uv);
  float texture=1.+(drift-.5)*.065;
  gl_FragColor=vec4(cloud.rgb*uTint.rgb*texture,cloud.a)*uTint.a;
}`;
const fireflyVertex = `attribute vec4 aParticle;
varying float vOpacity;
void main(){gl_Position=vec4(aParticle.xy,0.,1.);gl_PointSize=aParticle.z;vOpacity=aParticle.w;}`;
const fireflyFragment = `precision mediump float;
varying float vOpacity;
void main(){
  vec2 p=gl_PointCoord*2.-1.;float d=dot(p,p);
  float halo=exp(-d*5.)*.11+exp(-d*24.)*.30;
  float core=exp(-d*190.);
  vec3 color=vec3(1.,.58,.17)*halo+vec3(1.,.89,.52)*core;
  gl_FragColor=vec4(color*vOpacity,0.);
}`;

type Program = { handle: WebGLProgram; position: number; uniforms: Record<string, WebGLUniformLocation | null> };
type Puff = { x: number; y: number; size: number; depth: number; phase: number; tile: number; lift: number };
type Fly = { x: number; y: number; depth: number; phase: number; speed: number };
const smooth = (a: number, b: number, t: number) => { const x = Math.max(0, Math.min(1, (t - a) / (b - a))); return x * x * (3 - 2 * x); };

export class CloudRenderer {
  private gl: WebGLRenderingContext;
  private sky: Program;
  private cloud: Program;
  private firefly: Program;
  private mist: Program;
  private noise: WebGLTexture;
  private moon: WebGLTexture;
  private moonImage: HTMLImageElement | null = null;
  private moonLoaded = false;
  private quad: WebGLBuffer;
  private particles: WebGLBuffer;
  private atlas: WebGLTexture;
  private puffs: Puff[] = [];
  private flies: Fly[] = [];
  private particleData = new Float32Array(40 * 4);
  private width = 1;
  private height = 1;
  private cssWidth = 1;
  private cssHeight = 1;
  private dpr = 1;
  private scale = 1;
  private frameCount = 0;
  private slowFrames = 0;
  private previous = 0;
  private disposed = false;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl", { alpha: false, antialias: false, depth: false, stencil: false, powerPreference: "low-power", preserveDrawingBuffer: false });
    if (!gl) throw new Error("WebGL is unavailable");
    this.gl = gl;
    this.quad = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);
    this.noise = this.createNoise();
    this.moon = this.loadMoon();
    this.sky = this.program(screenVertex, skyFragment, ["uResolution", "uTime", "uEntrance", "uPointer", "uScroll", "uMoonRadius", "uNoise", "uMoon", "uMoonLoaded"]);
    this.cloud = this.program(cloudVertex, cloudFragment, ["uQuad", "uAspect", "uAngle", "uAtlas", "uTile", "uTint", "uNoise", "uTime", "uPhase"]);
    this.firefly = this.program(fireflyVertex, fireflyFragment, [], "aParticle");
    this.mist = this.program(screenVertex, mistFragment, ["uResolution", "uTime", "uEntrance", "uScroll", "uPointer", "uLayer", "uNoise"]);
    this.atlas = this.bakeClouds();
    this.particles = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.particles);
    gl.bufferData(gl.ARRAY_BUFFER, this.particleData.byteLength, gl.DYNAMIC_DRAW);
    // Deterministic positions keep the composition stable across remounts.
    let seed = 73921;
    const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
    for (let layer = 0; layer < 3; layer++) {
      for (let i = 0; i < 7; i++) {
        const x = (i - 3) * .235 + (random() - .5) * .09;
        const edge = Math.pow(Math.min(1, Math.abs(x) * 2), 1.7);
        this.puffs.push({ x, y: [-.405, -.505, -.625][layer] + edge * [.055, .10, .12][layer] + random() * .025, size: [.48, .69, .98][layer] * (.90 + random() * .20), depth: layer / 2, phase: random() * Math.PI * 2, tile: Math.floor(random() * 8), lift: [.69, .91, 1.20][layer] });
      }
    }
    for (let i = 0; i < 40; i++) this.flies.push({ x: random() - .5, y: -.37 + random() * .69, depth: .35 + random() * .65, phase: random() * Math.PI * 2, speed: .35 + random() * .45 });
  }

  private program(vertex: string, fragment: string, names: string[], attribute = "aPosition"): Program {
    const gl = this.gl;
    const compile = (type: number, source: string) => {
      const shader = gl.createShader(type)!;
      gl.shaderSource(shader, source); gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
        const message = gl.getShaderInfoLog(shader); gl.deleteShader(shader); throw new Error(message || "Dreamscape shader compilation failed");
      }
      return shader;
    };
    const vs = compile(gl.VERTEX_SHADER, vertex), fs = compile(gl.FRAGMENT_SHADER, fragment);
    const handle = gl.createProgram()!;
    gl.attachShader(handle, vs); gl.attachShader(handle, fs); gl.linkProgram(handle);
    gl.deleteShader(vs); gl.deleteShader(fs);
    if (!gl.getProgramParameter(handle, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(handle) || "Dreamscape shader linking failed");
    const uniforms: Record<string, WebGLUniformLocation | null> = {};
    for (const name of names) uniforms[name] = gl.getUniformLocation(handle, name);
    return { handle, position: gl.getAttribLocation(handle, attribute), uniforms };
  }

  private useQuad(program: Program) {
    const gl = this.gl;
    gl.useProgram(program.handle);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quad);
    gl.enableVertexAttribArray(program.position);
    gl.vertexAttribPointer(program.position, 2, gl.FLOAT, false, 0, 0);
  }

  private createNoise() {
    const gl=this.gl,texture=gl.createTexture()!;
    const random=new Uint8Array(256*256),data=new Uint8Array(256*256*4);
    let seed=17891;
    for(let i=0;i<random.length;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;random[i]=seed>>>24;}
    for(let y=0;y<256;y++)for(let x=0;x<256;x++){
      const i=(y*256+x)*4;
      data[i]=random[y*256+x];data[i+1]=random[((y+17)&255)*256+((x+37)&255)];data[i+2]=0;data[i+3]=255;
    }
    gl.bindTexture(gl.TEXTURE_2D,texture);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,256,256,0,gl.RGBA,gl.UNSIGNED_BYTE,data);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.REPEAT);
    return texture;
  }

  private loadMoon() {
    const gl=this.gl,texture=gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D,texture);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([180,180,180,255]));
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    const image=new Image();this.moonImage=image;
    image.onload=()=>{
      if(this.disposed)return;
      gl.activeTexture(gl.TEXTURE2);gl.bindTexture(gl.TEXTURE_2D,texture);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,gl.RGBA,gl.UNSIGNED_BYTE,image);
      gl.generateMipmap(gl.TEXTURE_2D);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR_MIPMAP_LINEAR);
      this.moonLoaded=true;
    };
    image.src=moonMapUrl;
    return texture;
  }

  private bakeClouds() {
    const gl = this.gl;
    const atlas = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, atlas);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 2048, 1024, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const target = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, target);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, atlas, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error("Cloud atlas framebuffer is unavailable");
    const bake = this.program(screenVertex, atlasFragment, ["uNoise"]);
    this.useQuad(bake);
    gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,this.noise);gl.uniform1i(bake.uniforms.uNoise,1);
    gl.viewport(0, 0, 2048, 1024);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.deleteFramebuffer(target);
    gl.deleteProgram(bake.handle);
    return atlas;
  }

  /** Called by ResizeObserver, never by the animation loop. */
  resize(width: number, height: number, dpr = 1) {
    this.cssWidth = Math.max(1, width); this.cssHeight = Math.max(1, height); this.dpr = Math.min(2, dpr);
    const ratio = Math.min(this.dpr, Math.sqrt(1_800_000 / (this.cssWidth * this.cssHeight))) * this.scale;
    this.width = Math.max(1, Math.round(this.cssWidth * ratio)); this.height = Math.max(1, Math.round(this.cssHeight * ratio));
    if (this.canvas.width !== this.width || this.canvas.height !== this.height) {
      this.canvas.width = this.width; this.canvas.height = this.height;
    }
    this.gl.viewport(0, 0, this.width, this.height);
  }

  render(state: DreamFrame) {
    if (this.disposed) return;
    const gl = this.gl, aspect = this.width / this.height;
    const time = state.reducedMotion ? 0 : state.time;
    const entrance = state.reducedMotion ? 10 : state.entrance;
    const scroll = Math.max(0, Math.min(1, state.scroll));
    const active = state.reducedMotion ? 0 : state.pointerActive;
    const px = state.pointerX * aspect * .5, py = state.pointerY * .5;
    gl.disable(gl.BLEND);
    this.useQuad(this.sky);
    let u = this.sky.uniforms;
    gl.uniform2f(u.uResolution, this.width, this.height); gl.uniform1f(u.uTime, time); gl.uniform1f(u.uEntrance, entrance); gl.uniform2f(u.uPointer, state.pointerX, state.pointerY); gl.uniform1f(u.uScroll, scroll); gl.uniform1f(u.uMoonRadius, Math.min(.16, aspect * .28, 210 / this.cssHeight));
    gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,this.noise);gl.uniform1i(u.uNoise,1);
    gl.activeTexture(gl.TEXTURE2);gl.bindTexture(gl.TEXTURE_2D,this.moon);gl.uniform1i(u.uMoon,2);gl.uniform1f(u.uMoonLoaded,this.moonLoaded?1:0);
    gl.drawArrays(gl.TRIANGLES, 0, 6);

    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    this.drawMist(state,0);
    this.useQuad(this.cloud); u = this.cloud.uniforms;
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, this.atlas); gl.uniform1i(u.uAtlas, 0); gl.uniform1f(u.uAspect, aspect);gl.uniform1i(u.uNoise,1);gl.uniform1f(u.uTime,time);
    const rise = smooth(0, 4.8, entrance), exit = smooth(.55, 1., scroll);
    const portraitScale = Math.min(1, .72 + aspect * .25);
    for (const puff of this.puffs) {
      const depth = puff.depth;
      const drift = Math.sin(time * (.032 + depth * .018) + puff.phase);
      let x = puff.x * aspect + drift * (.023 + depth * .027);
      let y = puff.y + puff.lift * (1 - rise) + Math.sin(time * .074 + puff.phase) * (.007 + depth * .004);
      const dx = x - px, dy = y - py;
      const influence = Math.exp(-(dx * dx + dy * dy) / (.13 + depth * .08)) * active * rise;
      x += (dx / (Math.abs(dx) + .16)) * influence * (.045 + depth * .035);
      y -= influence * .016;
      x += state.pointerX * active * (.006 + depth * .010) * rise;
      y += state.pointerY * active * (.003 + depth * .005) * rise;
      // As the document scrolls, the banks lift and part with depth, retaining
      // their volume until they naturally travel beyond the viewport.
      x += (puff.x + drift * .09) * scroll * (.46 + depth * .35);
      y += scroll * (.82 + depth * .29);
      const size = puff.size * portraitScale * (1 + (1 - rise) * .15);
      const stretch = 1.565 + Math.max(0, aspect - 1.7) * .25;
      gl.uniform4f(u.uQuad, x, y, size * stretch, size);
      gl.uniform1f(u.uAngle, Math.sin(time * .034 + puff.phase) * .017 + scroll * puff.x * .12);
      gl.uniform2f(u.uTile, puff.tile % 4, Math.floor(puff.tile / 4));
      gl.uniform1f(u.uPhase,puff.phase);
      const tint = .94 - depth * .09;
      gl.uniform4f(u.uTint, tint, tint, tint * 1.01, (depth === 0 ? .78 : .96) * (1 - exit * .84));
      gl.drawArrays(gl.TRIANGLES, 0, 6);
    }

    this.drawMist(state,1);

    // Forty warm lights use a single point-sprite draw call. Their luminous
    // cores stay visible after the logo entrance and gently gather near a hand.
    const ratio = this.width / this.cssWidth;
    const flyEntrance = smooth(.25, 2.0, entrance);
    for (let i = 0; i < this.flies.length; i++) {
      const fly = this.flies[i], phase = fly.phase;
      let x = fly.x * aspect + Math.sin(time * .15 * fly.speed + phase) * .047;
      let y = fly.y + Math.sin(time * .23 * fly.speed + phase * 2) * .029;
      const dx = px - x, dy = py - y;
      const influence = Math.exp(-(dx * dx + dy * dy) / .17) * active;
      x += dx * influence * .28 + Math.sin(time * .9 + phase) * influence * .011;
      y += dy * influence * .28 + Math.cos(time * .8 + phase) * influence * .011;
      x += Math.sin(phase) * scroll * .28;
      y += scroll * (.55 + fly.depth * .35);
      const index = i * 4;
      this.particleData[index] = x * 2 / aspect; this.particleData[index + 1] = y * 2;
      this.particleData[index + 2] = (24 + fly.depth * 26) * ratio;
      const pulse = .58 + .42 * Math.pow(.5 + .5 * Math.sin(time * fly.speed + phase), 2);
      this.particleData[index + 3] = pulse * (.60 + fly.depth * .4) * flyEntrance * (1 - exit);
    }
    gl.blendFunc(gl.ONE, gl.ONE);
    gl.useProgram(this.firefly.handle); gl.bindBuffer(gl.ARRAY_BUFFER, this.particles);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.particleData);
    gl.enableVertexAttribArray(this.firefly.position); gl.vertexAttribPointer(this.firefly.position, 4, gl.FLOAT, false, 0, 0);
    gl.drawArrays(gl.POINTS, 0, this.flies.length);
    gl.disable(gl.BLEND);
  }

  private drawMist(state: DreamFrame, layer: number) {
    const gl=this.gl;
    this.useQuad(this.mist);const u=this.mist.uniforms;
    gl.uniform2f(u.uResolution,this.width,this.height);gl.uniform1f(u.uTime,state.reducedMotion?0:state.time);
    gl.uniform1f(u.uEntrance,state.reducedMotion?10:state.entrance);gl.uniform1f(u.uScroll,state.scroll);gl.uniform1f(u.uLayer,layer);
    gl.uniform2f(u.uPointer,state.pointerX*state.pointerActive,state.pointerY*state.pointerActive);gl.uniform1i(u.uNoise,1);
    gl.drawArrays(gl.TRIANGLES,0,6);
  }

  /** Sustained slow frames reduce only the atmosphere resolution. One-way,
   * infrequent adaptation avoids oscillation and ignores suspended tabs. */
  sampleFrame(now: number) {
    if (this.previous && now - this.previous < 180) {
      this.frameCount++;
      if (now - this.previous > 25) this.slowFrames++;
      if (this.frameCount >= 180) {
        if (this.slowFrames > 100 && this.scale > .7) { this.scale = Math.max(.7, this.scale * .85); this.resize(this.cssWidth, this.cssHeight, this.dpr); }
        this.frameCount = 0; this.slowFrames = 0;
      }
    }
    this.previous = now;
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    const gl = this.gl;
    if(this.moonImage){this.moonImage.onload=null;this.moonImage=null;}
    gl.deleteTexture(this.noise);gl.deleteTexture(this.moon);gl.deleteProgram(this.mist.handle);
    gl.deleteTexture(this.atlas); gl.deleteBuffer(this.quad); gl.deleteBuffer(this.particles);
    gl.deleteProgram(this.sky.handle); gl.deleteProgram(this.cloud.handle); gl.deleteProgram(this.firefly.handle);
  }
}
