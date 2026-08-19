import { describe, expect, it } from "vitest";
import nativeConfig from "../src-tauri/tauri.conf.json";
import nativeSource from "../src-tauri/src/lib.rs?raw";

describe("native window compositing", () => {
  it("fills the usable display without desktop gutters", () => {
    const mainWindow = nativeConfig.app.windows.find(
      (window: { label?: string }) => window.label === "main",
    );

    expect(mainWindow).toMatchObject({
      maximized: true,
      center: false,
      decorations: false,
      shadow: false,
    });
  });

  it("keeps one active native blur material across focus changes", () => {
    expect(nativeSource).toContain("Effect::UnderWindowBackground");
    expect(nativeSource).toContain("EffectState::Active");
    expect(nativeSource).toContain("window.maximize()?");
  });
});
