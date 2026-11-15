import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, mediaInfo } from "./api";
import { appendMedia, liftDelete, rippleDelete, splitAt, updateClip } from "./edits";
import { ExportDialog } from "./ExportDialog";
import { FontsPanel } from "./FontsPanel";
import { registerFontFaces } from "./fontFaces";
import { useHistory } from "./history";
import { Home } from "./Home";
import { toggleLanguage, useT } from "./i18n";
import { MediaPanel, toItems, type MediaItem } from "./MediaPanel";
import { Preview } from "./Preview";
import { ClipProperties, TextProperties } from "./Properties";
import { TextPresets } from "./TextPresets";
import { Timeline } from "./Timeline";
import type { FontInfo, Project, TextLayer } from "./types";
import { AUDIO_EXT, IMAGE_EXT, VIDEO_EXT, allClips, fileName, formatTime, fpsValue, newId, newProject, normalize, projectDuration } from "./types";

type Tab = "media" | "text" | "fonts";
const MEDIA_EXT = new Set([...VIDEO_EXT, ...IMAGE_EXT, ...AUDIO_EXT]);

export function App() {
  const t = useT();
  const { present: project, commit, reset, undo, redo } = useHistory<Project>(null);
  const [path, setPath] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [fonts, setFonts] = useState<FontInfo[]>([]);
  const [media, setMedia] = useState<MediaItem[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [tab, setTab] = useState<Tab>("media");
  const [exporting, setExporting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const loadFonts = useCallback(() => {
    api
      .fonts()
      .then((f) => {
        setFonts(f);
        registerFontFaces(f);
      })
      .catch(() => {});
  }, []);
  useEffect(loadFonts, [loadFonts]);

  useEffect(() => {
    if (!notice) return;
    const id = setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(id);
  }, [notice]);

  // Every change goes to the engine (and its autosave) before the preview redraws.
  useEffect(() => {
    if (!project) return;
    api
      .update(project)
      .then(() => setRevision((r) => r + 1))
      .catch((e) => setNotice(`${t("autosaveFailed")}: ${e}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project]);

  const start = useCallback(
    (p: Project, at: string | null) => {
      const n = normalize(p);
      reset(n);
      setPath(at);
      setFrame(0);
      setSelected(null);
      setMedia(toItems([...new Set(allClips(n).map((c) => c.source))]));
      api.missing(n).then(setMissing).catch(() => setMissing([]));
    },
    [reset],
  );

  const openPath = useCallback(
    (p: string) =>
      api
        .open(p)
        .then((proj) => start(proj, p))
        .catch((e) => setNotice(`${t("openFailed")}: ${e}`)),
    [start, t],
  );

  const importPaths = useCallback((paths: string[]) => {
    const ok = paths.filter((p) => MEDIA_EXT.has(p.split(".").pop()?.toLowerCase() ?? ""));
    setMedia((m) => [...m, ...toItems(ok.filter((p) => !m.some((x) => x.path === p)))]);
  }, []);

  const addMedia = useCallback(
    async (item: MediaItem) => {
      if (!project) return;
      try {
        const seconds = item.kind === "image" ? 5 : (await mediaInfo(item.path)).duration;
        commit(appendMedia(project, item.path, item.kind, seconds));
      } catch (e) {
        setNotice(`${t("openFailed")}: ${fileName(item.path)} (${e})`);
      }
    },
    [project, commit, t],
  );

  // Files dropped on the window go to the media panel.
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") importPaths(e.payload.paths);
    });
    return () => {
      un.then((f) => f());
    };
  }, [importPaths]);

  const addText = useCallback(
    (preset: Omit<TextLayer, "id" | "start" | "duration">) => {
      if (!project) return;
      const layer: TextLayer = { ...preset, id: newId(), start: frame, duration: Math.round(fpsValue(project.fps) * 3) };
      commit({ ...project, texts: [...project.texts, layer] });
      setSelected(layer.id);
    },
    [project, frame, commit],
  );

  const families = useMemo(() => fonts.filter((f) => f.origin !== "system").map((f) => f.family), [fonts]);
  const quickText = useCallback(() => {
    const family = ["Cairo", "IBM Plex Sans Arabic"].find((f) => families.includes(f)) ?? families[0] ?? "Cairo";
    addText({
      text: "اكتب هنا",
      font_family: family,
      size: 0.06,
      weight: 700,
      color: [255, 255, 255, 255],
      x: 0.5,
      y: 0.8,
      box_width: 0.9,
      shadow: true,
      outline: 0,
      outline_color: [0, 0, 0, 255],
      background: [0, 0, 0, 0],
      align: "center",
    });
  }, [addText, families]);

  const saveProject = useCallback(
    async (as = false) => {
      if (!project) return;
      let target = as ? null : path;
      if (!target) {
        target = await saveDialog({
          defaultPath: `${project.name || t("untitled")}.ave`,
          filters: [{ name: "AVE", extensions: ["ave"] }],
        });
      }
      if (!target) return;
      await api
        .save(project, target)
        .then(() => {
          setPath(target);
          setNotice(t("saved"));
        })
        .catch((e) => setNotice(String(e)));
    },
    [project, path, t],
  );

  const relink = useCallback(async () => {
    if (!project) return;
    let next = project;
    for (const old of missing) {
      const picked = await openDialog({ title: fileName(old), filters: [{ name: fileName(old), extensions: [...MEDIA_EXT] }] });
      if (typeof picked !== "string") continue;
      next = { ...next, tracks: next.tracks.map((tr) => ({ ...tr, clips: tr.clips.map((c) => (c.source === old ? { ...c, source: picked } : c)) })) };
    }
    commit(next);
    api.missing(next).then(setMissing);
    setMedia(toItems([...new Set(allClips(next).map((c) => c.source))]));
  }, [project, missing, commit]);

  const selText = project?.texts.find((x) => x.id === selected) ?? null;
  const selClip = project ? allClips(project).find((c) => c.id === selected) ?? null : null;

  const deleteSelected = useCallback(
    (ripple: boolean) => {
      if (!project || !selected) return;
      if (selText) commit({ ...project, texts: project.texts.filter((x) => x.id !== selected) });
      else commit(ripple ? rippleDelete(project, selected) : liftDelete(project, selected));
      setSelected(null);
    },
    [project, selected, selText, commit],
  );

  // Keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement).closest("input, textarea, select");
      if (typing || !project || exporting) return;
      const mod = e.ctrlKey || e.metaKey;
      const end = projectDuration(project);
      const step = e.shiftKey ? Math.round(fpsValue(project.fps)) : 1;
      if (e.code === "Space") {
        e.preventDefault();
        setPlaying((p) => !p);
      } else if (mod && (e.code === "KeyY" || (e.code === "KeyZ" && e.shiftKey))) {
        e.preventDefault();
        redo();
      } else if (mod && e.code === "KeyZ") {
        e.preventDefault();
        undo();
      } else if (mod && e.code === "KeyS") {
        e.preventDefault();
        saveProject(e.shiftKey);
      } else if (mod && e.code === "KeyE") {
        e.preventDefault();
        setPlaying(false);
        setExporting(true);
      } else if (e.code === "KeyS" || (mod && e.code === "KeyB")) {
        commit(splitAt(project, frame, selClip?.id));
      } else if (e.code === "KeyT" && !mod) {
        quickText();
      } else if (e.code === "Delete" || e.code === "Backspace") {
        deleteSelected(!e.shiftKey);
      } else if (e.code === "ArrowLeft" || e.code === "ArrowRight") {
        e.preventDefault();
        // The timeline always runs left to right, like a clock, in both languages.
        const forward = e.code === "ArrowRight";
        setFrame((f) => Math.max(0, Math.min(end, f + (forward ? step : -step))));
      } else if (e.code === "Home") {
        setFrame(0);
      } else if (e.code === "End") {
        setFrame(end);
      } else if (e.code === "Escape") {
        setSelected(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project, frame, selClip, exporting, commit, undo, redo, saveProject, deleteSelected, quickText]);

  if (!project) {
    return (
      <>
        <Home
          onNew={(w, h) => start(newProject(t("untitled"), w, h), null)}
          onOpen={openPath}
          onRecover={(p) => start(p, null)}
        />
        {notice && <div className="toast">{notice}</div>}
      </>
    );
  }

  const total = projectDuration(project);
  return (
    <div className="app">
      <header className="topbar">
        <button
          onClick={() => {
            setPlaying(false);
            reset(null);
          }}
        >
          {t("home")}
        </button>
        <input
          className="project-name"
          dir="auto"
          aria-label={t("projectNameLabel")}
          value={project.name}
          onChange={(e) => commit({ ...project, name: e.target.value })}
        />
        <span className="muted" dir="ltr">
          {project.width}×{project.height}
        </span>
        <div className="spacer" />
        <button onClick={undo} title="Ctrl+Z">
          {t("undo")}
        </button>
        <button onClick={redo} title="Ctrl+Shift+Z">
          {t("redo")}
        </button>
        <button onClick={() => saveProject()} title="Ctrl+S">
          {t("save")}
        </button>
        <button onClick={toggleLanguage}>{t("language")}</button>
        <button
          className="primary"
          disabled={total === 0}
          title="Ctrl+E"
          onClick={() => {
            setPlaying(false);
            setExporting(true);
          }}
        >
          {t("export")}
        </button>
      </header>

      {missing.length > 0 && (
        <div className="banner warn">
          <span>
            {t("missing")}: {missing.map(fileName).join("، ")}
          </span>
          <button onClick={relink}>{t("relink")}</button>
        </div>
      )}

      <aside className="side left">
        <nav className="tabs" role="tablist">
          {(["media", "text", "fonts"] as const).map((k) => (
            <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
              {t(k)}
            </button>
          ))}
        </nav>
        <div className="side-scroll">
          {tab === "media" && <MediaPanel items={media} onImport={importPaths} onAdd={addMedia} />}
          {tab === "text" && <TextPresets families={families} onAdd={addText} />}
          {tab === "fonts" && (
            <FontsPanel
              fonts={fonts}
              current={selText?.font_family}
              onUse={selText ? (family) => commit({ ...project, texts: project.texts.map((x) => (x.id === selText.id ? { ...x, font_family: family } : x)) }) : undefined}
              onAddFont={(p) =>
                api
                  .addFont(p)
                  .then(loadFonts)
                  .catch((e) => setNotice(String(e)))
              }
            />
          )}
        </div>
      </aside>

      <main className="stage">
        <Preview
          project={project}
          revision={revision}
          frame={frame}
          playing={playing}
          onFrame={setFrame}
          onStop={() => setPlaying(false)}
        />
        <div className="transport">
          <button onClick={() => setFrame(0)} title="Home">
            {t("toStart")}
          </button>
          <button className="play" onClick={() => setPlaying((p) => !p)} disabled={total === 0} title={t("playPause")} aria-label={t("playPause")}>
            {playing ? "❚❚" : "▶"}
          </button>
          <span className="time" dir="ltr">
            {formatTime(frame, project.fps)} / {formatTime(total, project.fps)}
          </span>
          <div className="spacer" />
          <button onClick={() => commit(splitAt(project, frame, selClip?.id))} title="S">
            {t("split")}
          </button>
          <button onClick={quickText} title="T">
            {t("addText")}
          </button>
        </div>
      </main>

      <aside className="side right">
        <h3 className="side-title">{t("properties")}</h3>
        <div className="side-scroll">
          {selText && (
            <TextProperties
              layer={selText}
              fonts={fonts}
              fps={fpsValue(project.fps)}
              onChange={(nt) => commit({ ...project, texts: project.texts.map((x) => (x.id === nt.id ? nt : x)) })}
              onDelete={() => deleteSelected(false)}
            />
          )}
          {selClip && (
            <ClipProperties
              clip={selClip}
              fps={fpsValue(project.fps)}
              onChange={(c) => commit(updateClip(project, c))}
              onRipple={() => deleteSelected(true)}
              onLift={() => deleteSelected(false)}
            />
          )}
          {!selText && !selClip && (
            <div className="panel-body">
              <p className="hint">{total === 0 ? t("emptyHint") : t("selectHint")}</p>
              <p className="hint small">{t("shortcutsHint")}</p>
            </div>
          )}
        </div>
      </aside>

      <section className="timeline-area">
        <Timeline project={project} frame={frame} selected={selected} onSeek={setFrame} onSelect={setSelected} onCommit={commit} />
      </section>

      {exporting && <ExportDialog project={project} onClose={() => setExporting(false)} />}
      {notice && <div className="toast">{notice}</div>}
    </div>
  );
}
