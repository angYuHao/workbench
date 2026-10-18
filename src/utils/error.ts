export const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));
