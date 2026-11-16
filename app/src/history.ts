import { useCallback, useRef, useState } from "react";

/** Project state with unlimited undo / redo. */
export function useHistory<T>(initial: T | null) {
  const [present, setPresent] = useState<T | null>(initial);
  const past = useRef<T[]>([]);
  const future = useRef<T[]>([]);

  const commit = useCallback((next: T) => {
    setPresent((cur) => {
      if (cur !== null) past.current.push(cur);
      future.current = [];
      return next;
    });
  }, []);

  /** Replace without an undo step (e.g. loading a project). */
  const reset = useCallback((next: T | null) => {
    past.current = [];
    future.current = [];
    setPresent(next);
  }, []);

  const undo = useCallback(() => {
    setPresent((cur) => {
      const prev = past.current.pop();
      if (prev === undefined || cur === null) return cur;
      future.current.push(cur);
      return prev;
    });
  }, []);

  const redo = useCallback(() => {
    setPresent((cur) => {
      const next = future.current.pop();
      if (next === undefined || cur === null) return cur;
      past.current.push(cur);
      return next;
    });
  }, []);

  return { present, commit, reset, undo, redo };
}
