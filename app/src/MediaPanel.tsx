import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { mediaInfo, thumbnailOf } from "./api";
import { useT } from "./i18n";
import type { MediaKind } from "./types";
import { AUDIO_EXT, IMAGE_EXT, VIDEO_EXT, fileName, mediaKindOf } from "./types";

export interface MediaItem {
  path: string;
  kind: MediaKind;
}

interface Props {
  items: MediaItem[];
  onImport: (paths: string[]) => void;
  onAdd: (item: MediaItem) => void;
}

export function MediaPanel({ items, onImport, onAdd }: Props) {
  const t = useT();
  const pick = async () => {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Media", extensions: [...VIDEO_EXT, ...IMAGE_EXT, ...AUDIO_EXT] }],
    });
    if (picked) onImport(Array.isArray(picked) ? picked : [picked]);
  };
  return (
    <div className="panel-body">
      <button className="wide" onClick={pick}>
        {t("import")}
      </button>
      {items.length === 0 && <p className="hint">{t("importHint")}</p>}
      <div className="media-grid">
        {items.map((m) => (
          <MediaTile key={m.path} item={m} onAdd={() => onAdd(m)} label={t("addToTimeline")} />
        ))}
      </div>
    </div>
  );
}

function MediaTile({ item, onAdd, label }: { item: MediaItem; onAdd: () => void; label: string }) {
  const [thumb, setThumb] = useState<string | null>(null);
  const [len, setLen] = useState<string>("");
  useEffect(() => {
    if (item.kind !== "audio") thumbnailOf(item.path).then(setThumb).catch(() => {});
    if (item.kind !== "image")
      mediaInfo(item.path)
        .then((i) => setLen(`${Math.floor(i.duration / 60)}:${String(Math.round(i.duration % 60)).padStart(2, "0")}`))
        .catch(() => {});
  }, [item]);
  return (
    <div className={`media-tile ${item.kind}`} title={item.path} onDoubleClick={onAdd}>
      <div className="thumb" style={{ backgroundImage: thumb ? `url(${thumb})` : undefined }}>
        {item.kind === "audio" && <span className="note">♪</span>}
        {len && <span className="len">{len}</span>}
        <button className="add" title={label} onClick={onAdd}>
          +
        </button>
      </div>
      <span className="name" dir="auto">
        {fileName(item.path)}
      </span>
    </div>
  );
}

export const toItems = (paths: string[]): MediaItem[] => paths.map((p) => ({ path: p, kind: mediaKindOf(p) }));
