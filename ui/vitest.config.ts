import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // @testing-library/react registers its own `afterEach` cleanup, and only finds an
    // `afterEach` to register with when the globals are exposed. Without this, every
    // render in a file stacks up in the same document and a query that should match one
    // element matches four - which reads like a component bug and is not one.
    globals: true,
  },
});
