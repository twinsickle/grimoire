/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Mount the application into the page.","criteria":[{"id":"mounts-app","statement":"The application is rendered into the page's #root element, and the page holds nothing else of its own."}]}
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
