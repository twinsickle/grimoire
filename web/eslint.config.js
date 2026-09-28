// Frontend lint: typescript-eslint's recommended set plus the React hooks
// rules. `npm run lint` runs it; `npm run check` adds the type check.
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/", "node_modules/", "visual/out/"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // `ref` is Grimoire's word for a kind:id reference, fixed by the tools'
      // JSON; this compiler heuristic takes every `ref` for a React ref.
      "react-hooks/refs": "off",
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
    },
  },
  {
    files: ["visual/**/*.{ts,mjs}", "*.config.{ts,js}"],
    languageOptions: { globals: globals.node },
  },
);
