/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Mount the application into the page.","criteria":[{"id":"strict-mode","statement":"The app renders under StrictMode so effect misuse surfaces in development."},{"id":"theme-tokens","statement":"The one stylesheet loaded here defines the dark palette once as theme tokens, and every component names a colour by its token, never by a literal."}]}
 * ```
 */

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
