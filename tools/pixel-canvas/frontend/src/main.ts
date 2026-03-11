/**
 * Main entry point — parse config and launch the appropriate mode.
 */

import { parseUrlConfig, setupConfigForm } from "./config.js";
import { AudienceCanvas } from "./canvas.js";
import { PresenterMosaic } from "./mosaic.js";
import "./style.css";

function launch(): void {
  const config = parseUrlConfig();

  const form = document.getElementById("config-form") as HTMLFormElement;
  const app = document.getElementById("app")!;

  if (config) {
    // Auto-launch from URL parameters
    form.style.display = "none";
    app.style.display = "block";

    if (config.mode === "audience") {
      new AudienceCanvas(
        app,
        config.gateway,
        config.gridX ?? 0,
        config.gridY ?? 0
      );
    } else if (config.mode === "presenter") {
      new PresenterMosaic(
        app,
        config.gateway,
        config.cols ?? 4,
        config.rows ?? 4
      );
    }
  } else {
    // Show config form
    form.style.display = "block";
    app.style.display = "none";
    setupConfigForm(form);
  }
}

launch();
