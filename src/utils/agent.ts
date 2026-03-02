import {
  COMPLETION_KEYWORDS,
  COMPLETION_SHORT_KEYWORDS,
  COMPLETION_SHORT_MAX_LEN,
} from "../constants";

export function isTaskComplete(content: string): boolean {
  const lower = content.toLowerCase();
  return (
    COMPLETION_KEYWORDS.some((kw) => lower.includes(kw)) ||
    COMPLETION_SHORT_KEYWORDS.some(
      (kw) => lower.includes(kw) && lower.length < COMPLETION_SHORT_MAX_LEN
    )
  );
}
