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

// This shader only runs during construction, never in the animation loop.
// Overlapping, genuinely three-dimensional round lobes produce silver edges,
// lavender self-shadow and soft density falloff without a noisy height field.
const atlasFragment = `precision highp float;
varying vec2 vUv;
float hash(float p){return fract(sin(p*127.1+311.7)*43758.5453);}
vec4 lobe(vec3 p,vec3 center,vec3 radius){
  vec3 q=(p-center)/radius;
  float d=max(0.,1.-dot(q,q));
  return vec4(d*d*14.4,-57.6*d*q/radius);
}
vec4 field(vec3 p,float seed){
  vec3 q=p;
  p+=vec3(sin(q.y*17.+seed)*sin(q.z*14.),sin(q.x*19.+seed*.6)*sin(q.z*17.),sin(q.x*15.)*sin(q.y*18.+seed))*.018;
  vec4 d=lobe(p,vec3(0.,-.18,0.),vec3(1.35,.36,.54));
  for(int i=0;i<13;i++){
    float k=float(i),r=.35+hash(seed+k*4.3)*.21;
    float x=-1.12+mod(k,5.)*.55;
    float upper=1.-abs(x)*.56;
    float y=-.02+upper*.33+hash(k*9.7+seed)*.14;
    float z=(hash(k*3.8+seed)-.5)*.64;
    if(i>4){x=-1.14+(k-5.)*.325;y=-.10+hash(k+seed)*.24;z=.38+hash(k*2.3+seed)*.15;r*=.70+hash(k*6.+seed)*.16;}
    d+=lobe(p,vec3(x,y,z),vec3(r,r*(.86+hash(k+seed)*.2),r));
  }
  return d;
}
float density(vec3 p,float seed){return field(p,seed).x;}
void main(){
  vec2 tile=floor(vUv*vec2(4.,2.));
  float seed=tile.x+tile.y*4.+2.14;
  vec2 uv=fract(vUv*vec2(4.,2.));
  vec2 xy=vec2((uv.x-.5)*3.6,(uv.y-.5)*2.3+.13);
  vec4 sum=vec4(0.);
  vec3 light=normalize(vec3(-.5,.9,.55));
  for(int s=0;s<56;s++){
    vec3 p=vec3(xy,1.16-float(s)*.04143);
    vec4 volume=field(p,seed);
    float d=volume.x;
    if(d>.005){
      vec3 normal=normalize(-volume.yzw+vec3(0.,0.,.00001));
      float shadow=exp(-density(p+light*.25,seed)*.3-density(p+light*.60,seed)*.32);
      float diffuse=max(0.,dot(normal,light));
      float up=normal.y*.5+.5;
      vec3 ambient=mix(vec3(.19,.175,.265),vec3(.32,.30,.40),up);
      vec3 color=ambient+vec3(.44,.43,.48)*diffuse*(.62+.38*shadow);
      color+=vec3(.045,.04,.06)*pow(1.-max(0.,normal.z),2.);
      float a=1.-exp(-d*.17);
      sum.rgb+=(1.-sum.a)*color*a;
      sum.a+=(1.-sum.a)*a;
    }
    if(sum.a>.996)break;
  }
  // Transparent padding protects atlas tiles when bilinear sampling the edges.
  float border=smoothstep(0.,.015,min(min(uv.x,uv.y),min(1.-uv.x,1.-uv.y)));
  gl_FragColor=sum*border;
}`;

const skyFragment = `precision highp float;
varying vec2 vUv;
uniform vec2 uResolution;
uniform float uTime;
uniform float uEntrance;
uniform vec2 uPointer;
uniform float uScroll;
uniform float uMoonRadius;
float hash(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
float noise(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);return mix(mix(hash(i),hash(i+vec2(1.,0.)),f.x),mix(hash(i+vec2(0.,1.)),hash(i+1.),f.x),f.y);}
void main(){
  float aspect=uResolution.x/uResolution.y;
  vec2 p=(vUv-.5)*vec2(aspect,1.);
  vec3 sky=mix(vec3(.092,.065,.147),vec3(.012,.017,.041),smoothstep(0.,.92,vUv.y));
  float haze=exp(-pow((vUv.y-.27)*3.4,2.));
  sky+=vec3(.020,.013,.040)*haze;
  for(int j=0;j<2;j++){
    float layer=float(j);
    vec2 grid=(p+uPointer*(.0015+layer*.001))*(115.+layer*68.);
    vec2 cell=floor(grid),f=fract(grid)-.5;
    float seed=hash(cell+layer*7.3);
    vec2 offset=vec2(hash(cell+1.2),hash(cell+8.9))-.5;
    float r=length(f-offset*.7);
    float star=exp(-r*r*(120.+layer*100.))*step(.976,seed);
    float pulse=.7+.3*sin(uTime*(.25+seed*.4)+seed*120.);
    sky+=mix(vec3(.43,.48,.74),vec3(.9,.79,.61),seed)*star*pulse*.66;
  }
  float rise=smoothstep(.8,2.8,uEntrance);
  float fade=smoothstep(1.25,2.5,uEntrance)*(1.-smoothstep(6.5,8.5,uEntrance))*(1.-smoothstep(0.,.5,uScroll));
  float radius=uMoonRadius;
  vec2 moon=(p-vec2(0.,mix(-.25,.10,rise)))/radius;
  float r=length(moon);
  sky+=vec3(.16,.135,.24)*exp(-r*r*.26)*fade;
  sky+=vec3(.075,.077,.12)*exp(-r*r*1.4)*fade;
  if(r<1.){
    float z=sqrt(1.-r*r);
    float diffuse=.65+.35*max(0.,dot(vec3(moon,z),normalize(vec3(-.4,.55,1.3))));
    float maria=noise(moon*4.1+3.)*.6+noise(moon*11.3)*.26+noise(moon*27.)*.14;
    vec3 color=vec3(.84,.84,.94)*diffuse*(.80+maria*.23);
    sky=mix(sky,color,fade*(1.-smoothstep(.990,1.,r)));
  }
  sky=mix(sky,vec3(.28,.25,.37),.72*(1.-smoothstep(.3,2.8,uEntrance)));
  sky*=1.-.18*smoothstep(.25,.9,length(p*vec2(.55,1.)));
  gl_FragColor=vec4(sky,1.);
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
const cloudFragment = `precision mediump float;
varying vec2 vUv;
uniform sampler2D uAtlas;
uniform vec2 uTile;
uniform vec4 uTint;
void main(){
  vec2 uv=(uTile+clamp(vUv,vec2(.002),vec2(.998)))/vec2(4.,2.);
  vec4 cloud=texture2D(uAtlas,uv);
  gl_FragColor=vec4(cloud.rgb*uTint.rgb,cloud.a)*uTint.a;
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
    this.sky = this.program(screenVertex, skyFragment, ["uResolution", "uTime", "uEntrance", "uPointer", "uScroll", "uMoonRadius"]);
    this.cloud = this.program(cloudVertex, cloudFragment, ["uQuad", "uAspect", "uAngle", "uAtlas", "uTile", "uTint"]);
    this.firefly = this.program(fireflyVertex, fireflyFragment, [], "aParticle");
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
    const bake = this.program(screenVertex, atlasFragment, []);
    this.useQuad(bake);
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
    gl.drawArrays(gl.TRIANGLES, 0, 6);

    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    this.useQuad(this.cloud); u = this.cloud.uniforms;
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, this.atlas); gl.uniform1i(u.uAtlas, 0); gl.uniform1f(u.uAspect, aspect);
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
      const tint = 1.0 - depth * .09;
      gl.uniform4f(u.uTint, tint, tint, tint * 1.01, (depth === 0 ? .78 : .96) * (1 - exit * .84));
      gl.drawArrays(gl.TRIANGLES, 0, 6);
    }

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
    gl.deleteTexture(this.atlas); gl.deleteBuffer(this.quad); gl.deleteBuffer(this.particles);
    gl.deleteProgram(this.sky.handle); gl.deleteProgram(this.cloud.handle); gl.deleteProgram(this.firefly.handle);
  }
}
