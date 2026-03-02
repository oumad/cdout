import { useState, useCallback, useRef, useEffect } from "react";
import { ERROR_AUTO_DISMISS_MS } from "../constants";

export function useError() {
  const [error, setErrorState] = useState("");
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearError = useCallback(() => {
    setErrorState("");
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const showError = useCallback((msg: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setErrorState(msg);
    timerRef.current = setTimeout(() => {
      setErrorState("");
      timerRef.current = null;
    }, ERROR_AUTO_DISMISS_MS);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return { error, showError, clearError };
}
