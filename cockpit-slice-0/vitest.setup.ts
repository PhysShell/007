import "@testing-library/jest-dom/vitest";
import { afterEach, expect } from "vitest";
import { cleanup } from "@testing-library/react";
import { toHaveNoViolations } from "jest-axe";

// jest-axe ships a `toHaveNoViolations` matcher object; register it for a11y
// smoke checks. (It is already shaped as { toHaveNoViolations: fn }.)
expect.extend(toHaveNoViolations);

afterEach(() => {
  cleanup();
});
