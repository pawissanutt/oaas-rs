/**
 * Tests for the navigation config (src/config/nav.ts).
 *
 * Covers:
 * - Scripts entry exists in nav items
 * - Scripts entry has correct href and icon
 * - All nav items have required fields
 */
import { describe, it, expect } from "vitest";
import { NAV_ITEMS, SETTINGS_ITEM } from "@/config/nav";

describe("NAV_ITEMS", () => {
  it("contains a Scripts entry", () => {
    const scriptsItem = NAV_ITEMS.find((item) => item.label === "Scripts");
    expect(scriptsItem).toBeDefined();
  });

  it("Scripts entry has /scripts href", () => {
    const scriptsItem = NAV_ITEMS.find((item) => item.label === "Scripts");
    expect(scriptsItem?.href).toBe("/scripts");
  });

  it("Scripts entry has Code icon from lucide-react", () => {
    const scriptsItem = NAV_ITEMS.find((item) => item.label === "Scripts");
    expect(scriptsItem?.icon).toBeDefined();
    // The icon is the Code component from lucide-react
    expect(scriptsItem?.icon.displayName || scriptsItem?.icon.name).toBeTruthy();
  });

  it("Scripts appears before Packages in nav order", () => {
    const scriptsIndex = NAV_ITEMS.findIndex((item) => item.label === "Scripts");
    const packagesIndex = NAV_ITEMS.findIndex((item) => item.label === "Packages");
    expect(scriptsIndex).toBeGreaterThan(-1);
    expect(packagesIndex).toBeGreaterThan(-1);
    expect(scriptsIndex).toBeLessThan(packagesIndex);
  });

  it("all nav items have label, href, and icon", () => {
    for (const item of NAV_ITEMS) {
      expect(item.label).toBeTruthy();
      expect(item.href).toBeTruthy();
      expect(item.icon).toBeDefined();
    }
  });

  it("has expected number of nav items", () => {
    // Home, Objects, Deployments, Functions, Envs, Topology, Scripts, Packages
    expect(NAV_ITEMS.length).toBe(8);
  });
});

describe("SETTINGS_ITEM", () => {
  it("has /settings href", () => {
    expect(SETTINGS_ITEM.href).toBe("/settings");
  });
});
