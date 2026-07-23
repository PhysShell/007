/* Ambient declaration for jest-axe (ships no types). Script file — no top-level
 * import/export — so `declare module` is a pure ambient module declaration.
 * Test-only. */
declare module "jest-axe" {
  export function axe(
    html: Element | string,
    options?: Record<string, unknown>
  ): Promise<unknown>;
  export const toHaveNoViolations: {
    toHaveNoViolations(received: unknown): {
      pass: boolean;
      message: () => string;
    };
  };
  export function configureAxe(
    options?: Record<string, unknown>
  ): typeof axe;
}
