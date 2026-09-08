// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import { applyWindowMaterial } from "./theme";

describe("applyWindowMaterial", () => {
  beforeEach(() => {
    document.documentElement.className = "macos";
  });

  it("keeps the macOS layout class while switching to Solid", () => {
    applyWindowMaterial("solid");

    expect(document.documentElement.classList.contains("macos")).toBe(true);
    expect(document.documentElement.classList.contains("material-solid")).toBe(true);
    expect(
      document.documentElement.classList.contains("material-translucent"),
    ).toBe(false);
  });

  it("keeps translucent and Solid classes mutually exclusive", () => {
    applyWindowMaterial("blur");
    expect(
      document.documentElement.classList.contains("material-translucent"),
    ).toBe(true);
    expect(document.documentElement.classList.contains("material-solid")).toBe(false);

    applyWindowMaterial("solid");
    applyWindowMaterial("glass");
    expect(
      document.documentElement.classList.contains("material-translucent"),
    ).toBe(true);
    expect(document.documentElement.classList.contains("material-solid")).toBe(false);
  });
});
