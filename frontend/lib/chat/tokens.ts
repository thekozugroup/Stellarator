// Rough token estimator. ~4 chars/token in English; biased low to avoid overpromising.
export function estimateTokens(text: string): number {
  if (!text) return 0;
  return Math.max(1, Math.ceil(text.length / 4));
}
