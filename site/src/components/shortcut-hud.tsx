import { RevealText } from "./reveal-text";
import { useState, type KeyboardEvent } from "react";

type Modifier = "Letters" | "Ctrl" | "Shift" | "Alt";
const hudCommands: Record<Modifier, [string, string][]> = {
  Letters: [
    ["V", "Move"],
    ["A", "Node"],
    ["P", "Pen"],
    ["N", "Pencil"],
    ["T", "Type"],
    ["B", "Brush"],
  ],
  Ctrl: [
    ["S", "Save"],
    ["C", "Copy"],
    ["V", "Paste"],
    ["Z", "Undo"],
    ["T", "Transform"],
    ["/", "Toggle HUD"],
  ],
  Shift: [
    ["O", "Artboard"],
    ["J", "Heal"],
    ["[", "Softer brush"],
    ["]", "Harder brush"],
  ],
  Alt: [
    ["drag", "Clone & move"],
    ["Pen drag", "Break handle"],
    ["Heal click", "Sample source"],
  ],
};
export function ShortcutHud() {
  const [modifier, setModifier] = useState<Modifier>("Letters");
  const [tool, setTool] = useState("Pen");
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    setModifier(
      event.ctrlKey || event.metaKey
        ? "Ctrl"
        : event.shiftKey
          ? "Shift"
          : event.altKey
            ? "Alt"
            : "Letters",
    );
  }
  return (
    <section className="hud-section">
      <div className="shell section">
        <div className="section-heading" data-motion>
          <RevealText text="Shortcuts for the current tool" />
          <p>Pick a tool and hold Ctrl, Shift or Alt to see its shortcuts.</p>
        </div>
        <div
          className="hud-demo"
          data-motion
          tabIndex={0}
          onKeyDown={keys}
          onKeyUp={keys}
          onBlur={(event) => {
            if (
              !event.currentTarget.contains(event.relatedTarget as Node | null)
            )
              setModifier("Letters");
          }}
          aria-label="Interactive shortcut preview. Focus here and hold Control, Shift or Alt, or use the buttons."
        >
          <div className="hud-demo-top">
            <span>Tool</span>
            <div className="segment-control" aria-label="Preview tool">
              {["Pen", "Move", "Brush"].map((value) => (
                <button
                  type="button"
                  key={value}
                  aria-pressed={tool === value}
                  onClick={() => setTool(value)}
                >
                  {value}
                </button>
              ))}
            </div>
          </div>
          <div className="hud-drawing" aria-hidden="true">
            <svg viewBox="0 0 800 180">
              <path
                className="guide-line"
                d="M60 138H740M125 35V153M668 35V153"
              />
              <path
                className="draw-curve"
                d="M125 138C260 138 206 28 360 70S530 172 668 42"
              />
              <path className="handle-line" d="m267 43 186 54" />
              <circle cx="267" cy="43" r="4" />
              <circle cx="453" cy="97" r="4" />
              <rect x="354" y="64" width="12" height="12" />
              <rect x="119" y="132" width="12" height="12" />
              <rect x="662" y="36" width="12" height="12" />
            </svg>
          </div>
          <div className="hud-tool-row">
            <span>{tool}</span>
            {tool === "Pen" ? (
              <>
                <span>
                  <kbd>Shift</kbd> 45° angles
                </span>
                <span>
                  <kbd>Alt</kbd> Break handle
                </span>
                <span>
                  <kbd>Enter</kbd> Finish path
                </span>
              </>
            ) : tool === "Move" ? (
              <>
                <span>
                  <kbd>Shift</kbd> Constrain movement
                </span>
                <span>
                  <kbd>Alt</kbd> Clone & move
                </span>
                <span>
                  <kbd>Ctrl</kbd> Reverse snapping
                </span>
              </>
            ) : (
              <>
                <span>
                  <kbd>Shift</kbd> 45° stroke
                </span>
                <span>
                  <kbd>[</kbd>
                  <kbd>]</kbd> Brush size
                </span>
                <span>
                  <kbd>Shift + [ / ]</kbd> Hardness
                </span>
              </>
            )}
          </div>
          <div className="hud-key-row">
            <span>{modifier === "Letters" ? "Keys" : modifier}</span>
            {hudCommands[modifier].map(([key, hint]) => (
              <span key={key}>
                <kbd>{key}</kbd>
                {hint}
              </span>
            ))}
          </div>
        </div>
        <div className="hud-caption">
          <span>Focus the preview and hold a key, or choose a modifier.</span>
          <div className="modifier-buttons">
            {(["Letters", "Ctrl", "Shift", "Alt"] as Modifier[]).map((key) => (
              <button
                type="button"
                key={key}
                aria-pressed={modifier === key}
                onClick={() => setModifier(key)}
              >
                {key}
              </button>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
