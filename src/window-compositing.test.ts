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
    expect(nativeSource).toContain("NATIVE_MATERIAL_OPACITY: f64 = 0.78");
    expect(nativeSource).toContain("material_view.setAlphaValue(NATIVE_MATERIAL_OPACITY)");
    expect(nativeSource).toContain("tauri::WindowEvent::Focused(_)");
    expect(nativeSource).toContain("stabilize_native_material(&stable_window)");
    expect(nativeSource).toContain("window.maximize()?");
  });
});
