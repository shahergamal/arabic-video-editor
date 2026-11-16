import { useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, mediaInfo, thumbnailOf } from "./api";
import { useT, type TKey } from "./i18n";
import type { MediaItem } from "./MediaPanel";
import { MC_TRACK, angleOf, buildMulticam, switchAngle } from "./multicam";
import { label, reloadPlugins, usePlugins } from "./plugins";
import type { Angle, Project } from "./types";
import { fileName } from "./types";
import { useEffect } from "react";

export function MulticamSection({ project, media, frame, onCommit }: { project: Project; media: MediaItem[]; frame: number; onCommit: (p: Project) => void }) {
  const t = useT();
  const videos = media.filter((m) => m.kind === "video");
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const angles = project.angles ?? [];

  const sync = async () => {
    if (picked.length < 2) return setNote(t("needTwo"));
    setBusy(true);
    setNote(null);
    try {
      const synced = await api.syncAngles(picked);
      const lengths = await Promise.all(picked.map((p) => mediaInfo(p).then((i) => i.duration)));
      const next: Angle[] = synced.map((s, i) => ({ source: s.source, offset: s.offset, duration: lengths[i], name: "" }));
      onCommit(buildMulticam(project, next));
      const doubtful = synced.filter((s) => s.confidence < 0.3).map((s) => fileName(s.source));
      if (doubtful.length) setNote(`${t("syncDoubt")}: ${doubtful.join("، ")}`);
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h4>{t("multicam")}</h4>
      <p className="hint small">{t("multicamHint")}</p>
      {videos.length < 2 && <p className="hint small">{t("needTwo")}</p>}
      {videos.map((v) => (
        <label className="check" key={v.path}>
          <input type="checkbox" checked={picked.includes(v.path)} onChange={(e) => setPicked((p) => (e.target.checked ? [...p, v.path] : p.filter((x) => x !== v.path)))} />
          <span dir="auto">{fileName(v.path)}</span>
        </label>
      ))}
      <button className="primary" disabled={busy || picked.length < 2} onClick={sync}>
        {busy ? t("syncing") : t("syncNow")}
      </button>
      {angles.length > 0 && (
        <>
          <span className="label">{t("angles")}</span>
          <AngleButtons project={project} frame={frame} onCommit={onCommit} onRefused={() => setNote(t("angleUnavailable"))} />
          <p className="hint small">{t("switchHint")}</p>
        </>
      )}
      {note && (
        <p className="hint small" dir="auto">
          {note}
        </p>
      )}
    </>
  );
}

/** One button per camera, with a picture; the current one is lit. */
export function AngleButtons({ project, frame, onCommit, onRefused, compact }: { project: Project; frame: number; onCommit: (p: Project) => void; onRefused?: () => void; compact?: boolean }) {
  const angles = project.angles ?? [];
  const mc = project.tracks.find((t) => t.id === MC_TRACK);
  const now = mc?.clips.find((c) => frame >= c.start && frame < c.start + c.duration);
  const current = now ? angleOf(project, now) : -1;
  return (
    <div className={`angles ${compact ? "compact" : ""}`}>
      {angles.map((a, i) => (
        <button
          key={a.source}
          className={current === i ? "on" : ""}
          title={a.name}
          onClick={() => {
            const next = switchAngle(project, frame, i);
            if (next === project && current !== i) onRefused?.();
            else onCommit(next);
          }}
        >
          {!compact && <Thumb path={a.source} />}
          <span dir="ltr">{i + 1}</span>
        </button>
      ))}
    </div>
  );
}

function Thumb({ path }: { path: string }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    thumbnailOf(path).then(setUrl).catch(() => {});
  }, [path]);
  return <span className="thumb" style={{ backgroundImage: url ? `url(${url})` : undefined }} />;
}

export function PluginsSection() {
  const t = useT();
  const plugins = usePlugins();
  const [note, setNote] = useState<string | null>(null);
  const install = async () => {
    const folder = await openDialog({ directory: true });
    if (typeof folder !== "string") return;
    try {
      await api.installPlugin(folder);
      await reloadPlugins();
      setNote(t("pluginInstalled"));
    } catch (e) {
      setNote(String(e));
    }
  };
  return (
    <>
      <h4>{t("plugins")}</h4>
      <p className="hint small">{t("pluginsHint")}</p>
      {plugins.length === 0 && <p className="muted small">{t("noPlugins")}</p>}
      <ul className="issues">
        {plugins.map((p) => (
          <li key={p.id}>
            <span>
              {label(p)}{" "}
              <span className="muted small" dir="ltr">
                {p.version} {p.author && `· ${p.author}`} {p.license && `· ${p.license}`}
              </span>
            </span>
            <button onClick={() => api.removePlugin(p.id).then(reloadPlugins)}>{t("removePlugin")}</button>
          </li>
        ))}
      </ul>
      <button onClick={install}>{t("installPlugin")}</button>
      {note && (
        <p className="hint small" dir="auto">
          {note}
        </p>
      )}
    </>
  );
}

const FORMATS = [
  { id: "xml", key: "toPremiere", ext: "xml" },
  { id: "otio", key: "toOtio", ext: "otio" },
  { id: "edl", key: "toEdl", ext: "edl" },
] as const;

/** Save the timeline for Premiere, Resolve and other editors. */
export function ExchangeSection({ project }: { project: Project }) {
  const t = useT();
  const [note, setNote] = useState<string | null>(null);
  const save = async (f: (typeof FORMATS)[number]) => {
    const path = await saveDialog({ defaultPath: `${project.name || "timeline"}.${f.ext}`, filters: [{ name: f.ext.toUpperCase(), extensions: [f.ext] }] });
    if (!path) return;
    try {
      const lost = await api.exportTimeline(path, f.id);
      setNote(`✓ ${t("exchanged")}: ${fileName(path)}` + (lost.length ? `\n${t("lostIntro")} ${lost.map((k) => t(`lost_${k}` as TKey) ?? k).join("، ")}` : ""));
    } catch (e) {
      setNote(String(e));
    }
  };
  return (
    <>
      <span className="label">{t("exchange")}</span>
      <p className="hint small">{t("exchangeHint")}</p>
      <div className="chips">
        {FORMATS.map((f) => (
          <button key={f.id} onClick={() => save(f)}>
            {t(f.key)}
          </button>
        ))}
      </div>
      {note && (
        <p className="hint small pre" dir="auto">
          {note}
        </p>
      )}
    </>
  );
}
