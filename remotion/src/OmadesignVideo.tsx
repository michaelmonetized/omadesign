import {
  AbsoluteFill,
  Img,
  interpolate,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
  Sequence,
} from "remotion";

const CURL = "curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh";

// Mock pane component that looks like the Studio's right panel
const Pane: React.FC<{ title: string; children: React.ReactNode; accent?: string }> = ({
  title,
  children,
  accent = "#b4befe",
}) => (
  <div
    style={{
      width: 420,
      background: "#1e1e2e",
      border: "1px solid #313244",
      borderRadius: 16,
      overflow: "hidden",
      boxShadow: "0 20px 60px rgba(0,0,0,0.5)",
      fontFamily: '"IBM Plex Sans", sans-serif',
    }}
  >
    <div
      style={{
        padding: "12px 16px",
        borderBottom: "1px solid #313244",
        background: "#181825",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
      }}
    >
      <span style={{ fontSize: 13, fontWeight: 600, color: accent, letterSpacing: 0.5 }}>{title}</span>
      <span style={{ fontSize: 10, color: "#6c7086" }}>● ● ●</span>
    </div>
    <div style={{ padding: 16, color: "#cdd6f4", fontSize: 13, lineHeight: 1.5 }}>{children}</div>
  </div>
);

const Tag: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <span
    style={{
      display: "inline-block",
      background: "#313244",
      color: "#bac2de",
      fontSize: 10,
      padding: "2px 6px",
      borderRadius: 6,
      fontFamily: "JetBrains Mono, monospace",
      marginRight: 6,
    }}
  >
    {children}
  </span>
);

export const OmadesignVideo: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const titleSpring = spring({ frame, fps, config: { damping: 18, stiffness: 90 } });
  const titleY = interpolate(titleSpring, [0, 1], [60, 0]);
  const titleOpacity = interpolate(titleSpring, [0, 1], [0, 1]);

  return (
    <AbsoluteFill
      style={{
        background: "radial-gradient(1200px 800px at 30% 20%, #313244 0%, #1e1e2e 50%, #11111b 100%)",
        color: "#cdd6f4",
        fontFamily: '"Iosevka Aile", "IBM Plex Sans", sans-serif',
      }}
    >
      {/* Title card 0-90 */}
      <Sequence from={0} durationInFrames={90}>
        <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", display: "flex", flexDirection: "column" }}>
          <div
            style={{
              opacity: titleOpacity,
              transform: `translateY(${titleY}px)`,
              textAlign: "center",
            }}
          >
            <div style={{ fontSize: 14, letterSpacing: 4, color: "#7f849c", marginBottom: 16 }}>v0.0.0.0alpha-rc — 7 stacked PRs</div>
            <div style={{ fontSize: 84, fontWeight: 700, letterSpacing: -2, lineHeight: 1, color: "#cdd6f4" }}>
              omadesign
            </div>
            <div style={{ fontSize: 22, color: "#a6adc8", marginTop: 12, letterSpacing: 0.5 }}>design · paint · photograph — your Linux, for making things</div>
            <div style={{ marginTop: 32, display: "flex", gap: 12, justifyContent: "center" }}>
              <Tag>Free MIT</Tag>
              <Tag>aarch64 Asahi + x86_64</Tag>
              <Tag>glibc 2.35</Tag>
              <Tag>No Electron</Tag>
            </div>
            <div
              style={{
                marginTop: 36,
                background: "#181825",
                border: "1px solid #45475a",
                borderRadius: 12,
                padding: "12px 20px",
                fontFamily: "JetBrains Mono, monospace",
                fontSize: 15,
                color: "#a6e3a1",
              }}
            >
              {CURL}
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Design / Resize 90-180 */}
      <Sequence from={90} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>01 — DESIGN</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Resize from any tool</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              Handles now work from <b>Rect/Ellipse/Star/Line</b> — no more pressing <b>V</b> first.
              <br />
              <span style={{ color: "#cdd6f4" }}>Shift</span> uniform · <span style={{ color: "#cdd6f4" }}>Alt</span> from centre · <span style={{ color: "#cdd6f4" }}>15°</span> snap rotate
              <br />
              Group bbox + precise <b>X/Y/W/H/Rotate/Flip</b> in Transform.
            </div>
          </div>
          <div style={{ position: "relative", width: 720, height: 450, borderRadius: 16, overflow: "hidden", border: "1px solid #313244", background: "#181825" }}>
            <Img src={staticFile("media/design.jpg")} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
            <div style={{ position: "absolute", top: 12, left: 12, background: "#1e1e2e", border: "1px solid #313244", borderRadius: 8, padding: "6px 10px", fontSize: 11, color: "#bac2de" }}>
              ◈ 8 handles + rotate handle above — even on a fresh rect
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Fonts 180-270 */}
      <Sequence from={180} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ width: 720, height: 460, borderRadius: 16, overflow: "hidden", border: "1px solid #313244", background: "#181825", position: "relative" }}>
            <Img src={staticFile("media/type.jpg")} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
            <div style={{ position: "absolute", bottom: 0, left: 0, right: 0, background: "rgba(17,17,27,0.9)", padding: 12, borderTop: "1px solid #313244" }}>
              <div style={{ fontSize: 11, color: "#7f849c" }}>Character → Google Fonts on tap</div>
              <div style={{ fontSize: 13, color: "#cdd6f4", marginTop: 4 }}>Inter not installed? → Download & use in one click</div>
            </div>
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>02 — TYPE</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Fonts, full</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              <b>2000</b> cap via <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>fontconfig + ttf_parser</code>
              <br />
              Real family/style names.
              <br />
              <span style={{ color: "#b4befe" }}>Google Fonts browser</span> — 30 bundled, <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>~/.local/share/fonts/omadesign/google/</code>
              <br />
              <span style={{ color: "#b4befe" }}>Max font</span> scans <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>~/Projects</code> `next/font/google` → Inter as default.
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Palettes 270-360 */}
      <Sequence from={270} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>03 — COLOUR</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Custom palettes</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              Create, save, reuse at <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>~/.config/omadesign/palettes.json</code>
              <br />
              <b>+Fill / +Stroke / Clear / × Last</b> · Import/Export JSON · Recent still there.
            </div>
            <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
              {["#f38ba8", "#fab387", "#f9e2af", "#a6e3a1", "#89b4fa", "#cba6f7"].map((c) => (
                <div key={c} style={{ width: 48, height: 48, borderRadius: 8, background: c, border: "1px solid #313244" }} />
              ))}
            </div>
          </div>
          <Pane title="Palettes" accent="#f38ba8">
            <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
              <span style={{ flex: 1, background: "#313244", borderRadius: 6, padding: "6px 8px", fontSize: 12 }}>Oma Default ▾</span>
              <span style={{ background: "#45475a", borderRadius: 6, padding: "6px 10px", fontSize: 12 }}>＋ New</span>
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              {["#f38ba8", "#fab387", "#f9e2af", "#a6e3a1", "#94e2d5", "#89b4fa", "#74c7ec", "#cba6f7"].map((c) => (
                <div key={c} style={{ width: 22, height: 22, borderRadius: 4, background: c }} />
              ))}
            </div>
            <div style={{ marginTop: 12, display: "flex", gap: 6 }}>
              <span style={{ background: "#1e1e2e", border: "1px solid #313244", borderRadius: 6, padding: "4px 8px", fontSize: 11 }}>+ Fill</span>
              <span style={{ background: "#1e1e2e", border: "1px solid #313244", borderRadius: 6, padding: "4px 8px", fontSize: 11 }}>+ Stroke</span>
              <span style={{ background: "#1e1e2e", border: "1px solid #313244", borderRadius: 6, padding: "4px 8px", fontSize: 11 }}>Import…</span>
              <span style={{ background: "#1e1e2e", border: "1px solid #313244", borderRadius: 6, padding: "4px 8px", fontSize: 11 }}>Export…</span>
            </div>
          </Pane>
        </AbsoluteFill>
      </Sequence>

      {/* Compound 360-450 */}
      <Sequence from={360} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <Pane title="Compound / Pathfinder" accent="#a6e3a1">
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              <span style={{ background: "#313244", padding: "6px 12px", borderRadius: 8, fontSize: 12 }}>Union</span>
              <span style={{ background: "#313244", padding: "6px 12px", borderRadius: 8, fontSize: 12 }}>Subtract</span>
              <span style={{ background: "#313244", padding: "6px 12px", borderRadius: 8, fontSize: 12 }}>Intersect</span>
              <span style={{ background: "#313244", padding: "6px 12px", borderRadius: 8, fontSize: 12 }}>Xor</span>
            </div>
            <div style={{ marginTop: 12, display: "flex", gap: 6 }}>
              <span style={{ background: "#b4befe", color: "#1e1e2e", padding: "6px 14px", borderRadius: 8, fontSize: 12, fontWeight: 600 }}>Combine (Ctrl+E)</span>
              <span style={{ background: "#313244", padding: "6px 14px", borderRadius: 8, fontSize: 12 }}>Release</span>
            </div>
            <div style={{ marginTop: 10, fontSize: 11, color: "#7f849c" }}>Even-odd Poly · Explode back to parts · Multi-boolean folds</div>
          </Pane>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>04 — OBJECT</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Compound shapes</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              <b>Combine</b> concatenates world contours → even-odd <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>Poly</code>
              <br />
              <b>Release</b> explodes back. <b>Boolean</b> now folds across <b>N</b> shapes.
              <br />
              <span style={{ color: "#a6e3a1" }}>Ctrl+E</span> / <span style={{ color: "#a6e3a1" }}>Ctrl+Shift+E</span> + Object menu.
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Shape Browser 450-540 */}
      <Sequence from={450} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>05 — SHAPES</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Shape browser</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              OSS libs: <b>Phosphor Light</b> (well), <b>LineIcons</b>, <b>Heroicons</b>, <b>Feather</b>, <b>Lucide</b>
              <br />
              Live SVG fetch → <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>svg_to_geom</code> → Poly @ artboard centre.
              <br />
              <span style={{ color: "#b4befe" }}>◇ Shapes</span> in top bar or Welcome.
            </div>
          </div>
          <Pane title="Shape Browser — Phosphor" accent="#89b4fa">
            <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
              <span style={{ background: "#313244", borderRadius: 6, padding: "4px 8px", fontSize: 11 }}>All ▾</span>
              <span style={{ flex: 1, background: "#11111b", border: "1px solid #313244", borderRadius: 6, padding: "4px 8px", fontSize: 11, color: "#6c7086" }}>Search heart, arrow…</span>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 8 }}>
              {["house", "heart", "star", "gear", "camera", "globe", "bell", "user"].map((n) => (
                <div key={n} style={{ background: "#181825", border: "1px solid #313244", borderRadius: 8, padding: 8, textAlign: "center" }}>
                  <div style={{ height: 28, display: "flex", alignItems: "center", justifyContent: "center", color: "#cdd6f4", fontSize: 18 }}>◇</div>
                  <div style={{ fontSize: 10, color: "#a6adc8", marginTop: 4 }}>{n}</div>
                  <div style={{ marginTop: 6, background: "#b4befe", color: "#1e1e2e", borderRadius: 6, padding: "2px 0", fontSize: 10, fontWeight: 600 }}>+ Add</div>
                </div>
              ))}
            </div>
          </Pane>
        </AbsoluteFill>
      </Sequence>

      {/* Asset Browser 540-630 */}
      <Sequence from={540} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <Pane title="Free Asset Browser" accent="#fab387">
            <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
              <span style={{ background: "#313244", borderRadius: 6, padding: "4px 8px", fontSize: 11 }}>All ▾</span>
              <span style={{ flex: 1, background: "#11111b", border: "1px solid #313244", borderRadius: 6, padding: "4px 8px", fontSize: 11, color: "#6c7086" }}>nature, city…</span>
              <span style={{ background: "#b4befe", color: "#1e1e2e", borderRadius: 6, padding: "4px 10px", fontSize: 11, fontWeight: 600 }}>Search</span>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 8 }}>
              {[
                { prov: "Pixabay", tag: "forest" },
                { prov: "Pexels", tag: "city" },
                { prov: "Picsum", tag: "abstract" },
              ].map((a) => (
                <div key={a.prov} style={{ background: "#181825", border: "1px solid #313244", borderRadius: 8, overflow: "hidden" }}>
                  <div style={{ height: 64, background: a.prov === "Pixabay" ? "#ff6b6b" : a.prov === "Pexels" ? "#4ecdc4" : "#45b7d1" }} />
                  <div style={{ padding: 6 }}>
                    <div style={{ fontSize: 10, color: "#7f849c" }}>{a.prov}</div>
                    <div style={{ fontSize: 11, color: "#cdd6f4" }}>{a.tag}</div>
                    <div style={{ marginTop: 4, background: "#313244", borderRadius: 6, padding: "2px 0", textAlign: "center", fontSize: 10 }}>Add to artboard</div>
                  </div>
                </div>
              ))}
            </div>
            <div style={{ marginTop: 8, fontSize: 10, color: "#7f849c" }}>Picsum fallback when no API keys — always shows results</div>
          </Pane>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>06 — ASSETS</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Free assets</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              <b>Pixabay</b> + <b>Pexels</b> + <b>Vecteezy/Vexels</b> + <b>Picsum</b> fallback
              <br />
              Keys via <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>PIXABAY_API_KEY</code> or <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>~/.config/omadesign/assets.toml</code>
              <br />
              <span style={{ color: "#fab387" }}>⬙ Assets</span> → Search → Add as raster layer.
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Welcome 630-720 */}
      <Sequence from={630} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>07 — WELCOME</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Welcome 2.0</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              Big <b>720 px</b> square, tabs <b>All/Web/Print/Social/Photo/Identity</b>
              <br />
              Larger <b>210×112</b> cards with aspect preview.
              <br />
              <b>Transparent / Bleed / Safe</b> + <b>Artboards ×1–16</b> tiled.
            </div>
          </div>
          <div
            style={{
              width: 560,
              height: 420,
              background: "#1e1e2e",
              border: "1px solid #313244",
              borderRadius: 16,
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
            }}
          >
            <div style={{ padding: "14px 16px", borderBottom: "1px solid #313244", display: "flex", gap: 8, alignItems: "center" }}>
              {["All", "Web", "Print", "Social", "Photo", "Identity"].map((t, i) => (
                <div
                  key={t}
                  style={{
                    padding: "6px 12px",
                    borderRadius: 8,
                    fontSize: 11,
                    fontWeight: 600,
                    background: i === 1 ? "#b4befe" : "#313244",
                    color: i === 1 ? "#1e1e2e" : "#cdd6f4",
                  }}
                >
                  {t}
                </div>
              ))}
            </div>
            <div style={{ flex: 1, padding: 16, display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 10, background: "#181825" }}>
              {[
                { name: "HD 1920×1080", asp: 1.78 },
                { name: "4K", asp: 1.78 },
                { name: "Square 1080", asp: 1 },
              ].map((p) => (
                <div key={p.name} style={{ background: "#1e1e2e", border: "1px solid #313244", borderRadius: 10, padding: 10, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
                  <div style={{ width: 80, height: 80 / p.asp, background: "rgba(180,190,254,0.18)", border: "1px solid #b4befe", borderRadius: 6 }} />
                  <div style={{ fontSize: 10, color: "#cdd6f4", marginTop: 6 }}>{p.name}</div>
                </div>
              ))}
            </div>
            <div style={{ padding: 12, borderTop: "1px solid #313244", display: "flex", gap: 12, fontSize: 11, color: "#a6adc8" }}>
              <span>⬜ Transparent</span>
              <span>⬜ Bleed</span>
              <span>⬜ Safe</span>
              <span>× 4 artboards</span>
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Canvas artboards 720-810 */}
      <Sequence from={720} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ width: 720, height: 420, background: "#181825", border: "1px solid #313244", borderRadius: 16, position: "relative", overflow: "hidden" }}>
            {/* Checker */}
            <div style={{ position: "absolute", inset: 0, background: "repeating-conic-gradient(#313244 0% 25%, #1e1e2e 0% 50%) 0 0 / 32px 32px" }} />
            {/* Artboards */}
            <div style={{ position: "absolute", left: 40, top: 40, width: 300, height: 340, background: "#cdd6f4", border: "1px solid #6c7086", borderRadius: 2 }}>
              <div style={{ position: "absolute", inset: -6, border: "1px solid #f38ba8", opacity: 0.6 }} />
              <div style={{ position: "absolute", inset: 18, border: "1px solid #a6e3a1", opacity: 0.5, background: "rgba(166,227,161,0.08)" }} />
              <div style={{ position: "absolute", top: -20, left: 0, fontSize: 10, color: "#7f849c", fontFamily: "JetBrains Mono" }}>Artboard 1</div>
            </div>
            <div style={{ position: "absolute", left: 380, top: 40, width: 300, height: 340, background: "#cdd6f4", border: "1px solid #6c7086", borderRadius: 2 }}>
              <div style={{ position: "absolute", inset: -6, border: "1px solid #f38ba8", opacity: 0.6 }} />
              <div style={{ position: "absolute", inset: 18, border: "1px solid #a6e3a1", opacity: 0.5, background: "rgba(166,227,161,0.08)" }} />
              <div style={{ position: "absolute", top: -20, left: 0, fontSize: 10, color: "#7f849c", fontFamily: "JetBrains Mono" }}>Artboard 2</div>
            </div>
            <div style={{ position: "absolute", left: 360, top: 180, width: 8, height: 40, background: "#313244", borderRadius: 2 }} />
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>08 — CANVAS</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>Artboards + bleed</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              Tiled <b>48 px</b> gutter, <b>36 px</b> bleed (red) + crop marks,
              <br />
              safe <b>18 px</b> inset (green). Checker when transparent.
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Lander single HTML 810-900 */}
      <Sequence from={810} durationInFrames={90}>
        <AbsoluteFill style={{ display: "flex", alignItems: "center", padding: 60, gap: 40 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, letterSpacing: 3, color: "#7f849c", marginBottom: 12 }}>09 — LANDER</div>
            <div style={{ fontSize: 48, fontWeight: 700, lineHeight: 1 }}>One-file lander</div>
            <div style={{ fontSize: 18, color: "#a6adc8", marginTop: 12, lineHeight: 1.5 }}>
              Distilled to <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>index.html</code> + Tailwind CDN + palette in <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>&lt;style&gt;</code>
              <br />
              No Vite build, media at <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>media/</code> — also at <code style={{ background: "#313244", padding: "2px 6px", borderRadius: 6, fontSize: 13 }}>site/dist/</code>
            </div>
          </div>
          <div style={{ width: 720, height: 420, background: "#1e1e2e", border: "1px solid #313244", borderRadius: 16, overflow: "hidden", display: "flex", flexDirection: "column" }}>
            <div style={{ height: 32, background: "#181825", borderBottom: "1px solid #313244", display: "flex", alignItems: "center", padding: "0 12px", gap: 6 }}>
              <div style={{ width: 10, height: 10, borderRadius: 5, background: "#f38ba8" }} />
              <div style={{ width: 10, height: 10, borderRadius: 5, background: "#f9e2af" }} />
              <div style={{ width: 10, height: 10, borderRadius: 5, background: "#a6e3a1" }} />
              <span style={{ marginLeft: 12, fontSize: 11, color: "#6c7086", fontFamily: "JetBrains Mono" }}>index.html — 16 KB</span>
            </div>
            <div style={{ flex: 1, padding: 20, background: "#1e1e2e" }}>
              <div style={{ height: 14, background: "#313244", borderRadius: 4, width: "40%", marginBottom: 12 }} />
              <div style={{ height: 32, background: "#cdd6f4", borderRadius: 8, width: "70%", marginBottom: 16 }} />
              <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
                {[1, 2, 3, 4, 5, 6].map((i) => (
                  <div key={i} style={{ height: 80, background: "#181825", border: "1px solid #313244", borderRadius: 8 }} />
                ))}
              </div>
            </div>
          </div>
        </AbsoluteFill>
      </Sequence>

      {/* Outro 900-1350 */}
      <Sequence from={900} durationInFrames={450}>
        <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", display: "flex", flexDirection: "column", textAlign: "center" }}>
          <div style={{ fontSize: 64, fontWeight: 700, letterSpacing: -1 }}>omadesign</div>
          <div style={{ fontSize: 16, letterSpacing: 4, color: "#7f849c", marginTop: 8 }}>v0.0.0.0alpha-rc — glibc 2.35 · aarch64 + x86_64</div>
          <div
            style={{
              marginTop: 24,
              background: "#181825",
              border: "1px solid #45475a",
              borderRadius: 12,
              padding: "14px 22px",
              fontFamily: "JetBrains Mono, monospace",
              fontSize: 15,
              color: "#a6e3a1",
              maxWidth: 760,
            }}
          >
            curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
          </div>
          <div style={{ marginTop: 16, display: "flex", gap: 12 }}>
            <span style={{ background: "#b4befe", color: "#1e1e2e", padding: "8px 18px", borderRadius: 999, fontSize: 13, fontWeight: 600 }}>aarch64 Asahi</span>
            <span style={{ background: "#313244", color: "#cdd6f4", padding: "8px 18px", borderRadius: 999, fontSize: 13 }}>x86_64</span>
            <span style={{ border: "1px solid #45475a", color: "#a6adc8", padding: "8px 18px", borderRadius: 999, fontSize: 13 }}>github.com/michaelmonetized/omadesign</span>
          </div>
          <div style={{ marginTop: 24, fontSize: 13, color: "#6c7086" }}>
            GitHub Pages <b>https://michaelmonetized.github.io/omadesign/</b> · Vercel <b>michaelchurley.com/omadesign</b> (iframe)
          </div>
        </AbsoluteFill>
      </Sequence>
    </AbsoluteFill>
  );
};
