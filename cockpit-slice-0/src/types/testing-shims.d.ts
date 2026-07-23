/* Vitest matcher augmentation for jest-axe's `toHaveNoViolations`. Module file
 * (has `export {}`) so `declare module "vitest"` AUGMENTS the real vitest types
 * rather than replacing them. Test-only. */
import "vitest";

interface CustomAxeMatchers<R = unknown> {
  toHaveNoViolations(): R;
}

declare module "vitest" {
  // eslint-disable-next-line @typescript-eslint/no-empty-interface
  interface Assertion<T = unknown> extends CustomAxeMatchers<T> {}
  // eslint-disable-next-line @typescript-eslint/no-empty-interface
  interface AsymmetricMatchersContaining extends CustomAxeMatchers {}
}
