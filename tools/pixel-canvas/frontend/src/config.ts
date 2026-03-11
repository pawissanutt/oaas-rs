/**
 * Config — URL parameter parsing and config form management.
 */

import type { AppConfig } from "./types.js";

/** Read a URL search parameter. */
function param(name: string): string {
  return new URLSearchParams(location.search).get(name) ?? "";
}

/** Try to parse config from URL parameters. Returns null if incomplete. */
export function parseUrlConfig(): AppConfig | null {
  const mode = param("mode");
  const gateway = param("gateway");
  if (!mode || !gateway) return null;

  if (mode === "audience") {
    const grid = param("grid") || "0-0";
    const [gx, gy] = grid.split("-").map(Number);
    return { mode: "audience", gateway, gridX: gx, gridY: gy };
  } else if (mode === "presenter") {
    return {
      mode: "presenter",
      gateway,
      cols: Number(param("cols")) || 4,
      rows: Number(param("rows")) || 4,
    };
  }
  return null;
}

/** Wire up the config form and return a Promise that resolves when the user submits. */
export function setupConfigForm(form: HTMLFormElement): void {
  const audienceFields = document.getElementById("audience-fields")!;
  const presenterFields = document.getElementById("presenter-fields")!;

  function updateFieldVisibility(): void {
    const m = (
      form.querySelector(
        'input[name="mode"]:checked'
      ) as HTMLInputElement | null
    )?.value;
    audienceFields.style.display = m === "audience" ? "block" : "none";
    presenterFields.style.display = m === "presenter" ? "block" : "none";
  }

  form.querySelectorAll<HTMLInputElement>('input[name="mode"]').forEach((r) =>
    r.addEventListener("change", updateFieldVisibility)
  );
  updateFieldVisibility();

  // Pre-fill from existing URL params
  const gateway = param("gateway");
  if (gateway)
    (document.getElementById("gateway") as HTMLInputElement).value = gateway;
  if (param("grid"))
    (document.getElementById("grid") as HTMLInputElement).value = param("grid");
  if (param("cols"))
    (document.getElementById("cols") as HTMLInputElement).value = param("cols");
  if (param("rows"))
    (document.getElementById("rows") as HTMLInputElement).value = param("rows");

  const mode = param("mode");
  if (mode) {
    const r = form.querySelector<HTMLInputElement>(
      `input[name="mode"][value="${mode}"]`
    );
    if (r) {
      r.checked = true;
      updateFieldVisibility();
    }
  }

  // Form submit → rebuild URL with params
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const m = (
      form.querySelector(
        'input[name="mode"]:checked'
      ) as HTMLInputElement
    ).value;
    const gw = (
      document.getElementById("gateway") as HTMLInputElement
    ).value.trim();
    if (!gw) {
      alert("Gateway URL is required.");
      return;
    }

    const p = new URLSearchParams({ mode: m, gateway: gw });
    if (m === "audience") {
      p.set(
        "grid",
        (document.getElementById("grid") as HTMLInputElement).value.trim() ||
          "0-0"
      );
    } else {
      p.set(
        "cols",
        (document.getElementById("cols") as HTMLInputElement).value || "4"
      );
      p.set(
        "rows",
        (document.getElementById("rows") as HTMLInputElement).value || "4"
      );
    }
    location.search = p.toString();
  });
}
