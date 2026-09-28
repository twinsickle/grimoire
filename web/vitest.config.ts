import { defineConfig } from "vitest/config";

// Unit tests for the UI's logic and source rules; the views are tested in a
// browser by web/behaviour and web/visual, which Vitest must not pick up.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    // Load the stylesheet's text for the token test rather than stubbing it.
    css: { include: [/styles\.css/] },
  },
});
