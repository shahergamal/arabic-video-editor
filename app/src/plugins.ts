// Installed plugins, loaded once and shared by every panel.
import { useEffect, useState } from "react";
import { api, type Plugin } from "./api";

let cache: Plugin[] = [];
const listeners = new Set<(p: Plugin[]) => void>();

export function reloadPlugins() {
  return api
    .plugins()
    .then((p) => {
      cache = p;
      listeners.forEach((l) => l(p));
    })
    .catch(() => {});
}

export function usePlugins(): Plugin[] {
  const [list, setList] = useState(cache);
  useEffect(() => {
    listeners.add(setList);
    if (!cache.length) reloadPlugins();
    return () => {
      listeners.delete(setList);
    };
  }, []);
  return list;
}

/** A plugin's own name in the app's language. */
export const label = (x: { name: string; name_ar?: string | null }) => (document.documentElement.lang === "ar" && x.name_ar ? x.name_ar : x.name);
