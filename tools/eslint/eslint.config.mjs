// ESLint for the app's TypeScript and React code.
//
// Run from the repository root:  pnpm --dir app lint
// The `lint` script installs this package (tools/eslint) and runs ESLint with
// app/ as the working directory, so the patterns below are relative to app/.
//
// Why a separate package: the app builds with TypeScript 7, but typescript-eslint
// still needs the TypeScript 6 compiler API (peer range <6.1). Installing the
// lint tools here keeps that older TypeScript away from the app's `tsc`.
//
// Rules: ESLint and typescript-eslint "recommended" (no type information), plus
// the two classic React hooks rules. React hooks mistakes caused real bugs here
// (a stale value in a handler, a timer left running after unmount, a selector
// that returned a new object each call). Warnings fail too: the `lint` script
// passes --max-warnings 0, and CI runs it.

import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default defineConfig([
  {
    // Build output, the Rust side and the TypeScript generated from Rust.
    ignores: ["dist/", "node_modules/", "src-tauri/", "src/bindings/"],
  },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: "module",
      globals: globals.browser,
    },
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  },
  {
    files: ["vite.config.ts"],
    languageOptions: { globals: globals.node },
  },
]);
