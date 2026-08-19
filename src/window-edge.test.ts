import { describe, expect, it } from "vitest";
import nativeSource from "../src-tauri/src/lib.rs?raw";

describe("native window edge", () => {
  it("uses a square native material surface without a visible top radius", () => {
    expect(nativeSource).toContain(".radius(0.0)");
  });
});
