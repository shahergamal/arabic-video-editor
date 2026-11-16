import { useEffect, useState } from "react";
import { api } from "./api";
import { useT } from "./i18n";

/** Waveform, histogram and vectorscope of the frame under the playhead. */
export function Scopes({ frame, revision, playing }: { frame: number; revision: number; playing: boolean }) {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (playing) return;
    let live = true;
    const id = setTimeout(() => {
      api
        .scopes(frame)
        .then((b) => {
          if (!live) return;
          const next = URL.createObjectURL(new Blob([new Uint8Array(b)], { type: "image/jpeg" }));
          setUrl((old) => {
            if (old) URL.revokeObjectURL(old);
            return next;
          });
        })
        .catch(() => {});
    }, 120);
    return () => {
      live = false;
      clearTimeout(id);
    };
  }, [frame, revision, playing]);
  return (
    <div className="scopes" aria-label={t("scopesOn")}>
      {url ? <img src={url} alt={t("scopesOn")} /> : <span className="muted small">…</span>}
      <div className="scope-labels muted small" dir="ltr">
        <span>Waveform</span>
        <span>Histogram</span>
        <span>Vectorscope</span>
      </div>
    </div>
  );
}
