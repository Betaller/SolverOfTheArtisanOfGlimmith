// ESLint flat config — the JS/TS/Vue half of the project's detekt-style
// complexity gate (the Python half lives in scripts/complexity_gate.py).
//
// The load-bearing rule is `complexity`: cyclomatic complexity per function,
// capped at 10 — the same threshold detekt's CyclomaticComplexMethod uses by
// default, and the same cap `scripts/complexity_gate.py` enforces on src/ and
// scripts/.  Run via:  npm run lint   (see package.json).
import js from "@eslint/js";
import globals from "globals";
import pluginVue from "eslint-plugin-vue";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "src/wasm/**", "public/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs["flat/essential"],
  {
    files: ["**/*.{js,mjs,ts,vue}"],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
      globals: { ...globals.browser, ...globals.worker, ...globals.node },
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: [".vue"],
      },
    },
    rules: {
      // ── the gate ──────────────────────────────────────────────────────────
      complexity: ["error", { max: 10 }],
      // Nesting is what actually makes the remaining complex functions hard to
      // read; kept as a warning so it guides refactors without blocking a push.
      "max-depth": ["warn", { max: 4 }],
      // ── pragmatic overrides for a Vite/Vue codebase ───────────────────────
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_" }],
      "no-undef": "off", // TS/vue-tsc already covers this
      "vue/multi-word-component-names": "off",
    },
  },
);
