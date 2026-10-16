import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  cellProtocol,
  changesProtocol,
  preflightCreate,
  requireProtocol,
} from "../shared/liveProtocol";
const fixtures = JSON.parse(
  readFileSync(
    new URL("../shared/liveProtocol.fixtures.json", import.meta.url),
    "utf8",
  ),
);
describe("Canonical retained protocol policy", () => {
  for (const c of fixtures)
    it(c.name, () => {
      if (c.error)
        expect(() => cellProtocol(c.path, c.cell)).toThrow("Unsupported");
      else expect(cellProtocol(c.path, c.cell)).toBe(c.required);
    });
  it("does not promote compatible rooms merely because a peer offers 3", () => {
    expect(requireProtocol(3, 1)).toBe(3);
    expect(requireProtocol(2, 2)).toBe(2);
    expect(requireProtocol(undefined, 1)).toBe(1);
    for (const offer of [0, 4, NaN, "3", null])
      expect(() => requireProtocol(offer, 1)).toThrow("Unsupported");
  });
  it("classifies every retained write independently of deletion, toggle or array order", () => {
    const advanced = {
      edit: {
        type: "write",
        writes: [{ path: "/motion/scene3d", value: { Value: { nodes: [] } } }],
      },
    };
    const remove = {
      edit: {
        type: "write",
        writes: [{ path: "/motion/scene3d", value: null }],
      },
    };
    const undo = { edit: { type: "toggle", enabled: false } };
    expect(changesProtocol([advanced, remove, undo])).toBe(3);
    expect(changesProtocol([undo, remove, advanced])).toBe(3);
  });
  it("refuses old create offers before the caller can initialize quota", () => {
    for (const c of fixtures.filter((c: any) => c.required === 3)) {
      const seed = JSON.stringify({ "": "Object", [c.path]: c.cell });
      if (c.path === "") continue;
      expect(() => preflightCreate(seed, 2)).toThrow("protocol 3");
      expect(preflightCreate(seed, 3)).toBe(3);
    }
  });
  it("refuses unknown cells and excessive nesting without assigning Motion capability", () => {
    expect(() => cellProtocol("/name", { Future: 1 })).toThrow("Unsupported");
    let value: any = 1;
    for (let n = 0; n < 130; n++) value = { child: value };
    expect(() => cellProtocol("/name", { Value: value })).toThrow("nesting");
  });
});
