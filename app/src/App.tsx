import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, mediaInfo } from "./api";
import { addTrack, appendMedia, liftDelete, makeClip, place, rippleDelete, splitAt, updateClip } from "./edits";
import { BrandPanel, useBrand } from "./BrandPanel";
import { ExportDialog } from "./ExportDialog";
import { FontsPanel } from "./FontsPanel";
import { registerFontFaces } from "./fontFaces";
import { useHistory } from "./history";
import { Home } from "./Home";
import { toggleLanguage, useT } from "./i18n";
import { MediaPanel, toItems, type MediaItem } from "./MediaPanel";
import { Preview } from "./Preview";
import { CaptionsPanel } from "./CaptionsPanel";
import { AssistantPanel } from "./AssistantPanel";
import { Scopes } from "./Scopes";
import { AngleButtons } from "./Phase4Panels";
import { switchAngle } from "./multicam";
import { ClipProperties, TextProperties } from "./Properties";
import { TextPresets } from "./TextPresets";
import { Timeline } from "./Timeline";
import { addMarker, closeGaps, detachAudio, duplicate, freezeFrame, group, nextStop, paste, ungroup } from "./edits5";
import { AutoEditPanel, BeatSection, ReframeSection, ShapeProperties, ShapesPanel, UpdateBanner, VoiceRecorder } from "./Phase5Panels";
import { PublishDialog } from "./PublishDialog";
import { Section } from "./Properties";
import type { Clip, FontInfo, Project, TextLayer } from "./types";
import { AUDIO_EXT, IMAGE_EXT, VIDEO_EXT, allClips, fileName, formatTime, fpsValue, newId, newProject, normalize, projectDuration } from "./types";

type Tab = "media" | "text" | "captions" | "auto" | "fonts" | "assistant";
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
  const [showScopes, setShowScopes] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [brand, setBrand] = useBrand();
  const [templateName, setTemplateName] = useState<string | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [clipboard, setClipboard] = useState<Clip | null>(null);
  const [publishing, setPublishing] = useState(false);
  // J/K/L: negative plays backwards, 1 is normal, 2 and 4 are faster.
  const [rate, setRate] = useState(1);

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

  const addLogo = useCallback(
    (path: string) => {
      if (!project) return;
      const withTrack = addTrack(project, "video");
      const top = withTrack.tracks.filter((tr) => tr.kind === "video").pop()!;
      const fps = fpsValue(project.fps);
      const clip = makeClip(withTrack, path, "image", Math.max(3, projectDuration(project) / fps), 0);
      commit(place(withTrack, top.id, { ...clip, transform: { scale: 0.18, x: 0.36, y: -0.4, opacity: 1 } }));
      setSelected(clip.id);
    },
    [project, commit],
  );

  const families = useMemo(() => fonts.filter((f) => f.origin !== "system").map((f) => f.family), [fonts]);
  const quickText = useCallback(() => {
    const family = [brand.font ?? "", "Cairo", "IBM Plex Sans Arabic"].find((f) => families.includes(f)) ?? families[0] ?? "Cairo";
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
  }, [addText, families, brand.font]);

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
  const selShape = project?.shapes?.find((x) => x.id === selected) ?? null;
  const selClip = project ? allClips(project).find((c) => c.id === selected) ?? null : null;

  const deleteSelected = useCallback(
    (ripple: boolean) => {
      if (!project || !selected) return;
      if (selText) commit({ ...project, texts: project.texts.filter((x) => x.id !== selected) });
      else if (selShape) commit({ ...project, shapes: (project.shapes ?? []).filter((x) => x.id !== selected) });
      else {
        // Every picked clip goes, last first so ripples don't move the others' places.
        const ids = [...new Set([selected, ...picked])];
        const starts = new Map(allClips(project).map((c) => [c.id, c.start]));
        let next = project;
        for (const id of ids.sort((a, b) => (starts.get(b) ?? 0) - (starts.get(a) ?? 0))) next = ripple ? rippleDelete(next, id) : liftDelete(next, id);
        commit(next);
      }
      setSelected(null);
      setPicked([]);
    },
    [project, selected, selText, selShape, picked, commit],
  );

  const groupPicked = useCallback(() => {
    if (!project) return;
    const ids = [...new Set([...(selected ? [selected] : []), ...picked])];
    const next = group(project, ids, `${t("group")} ${(project.compounds?.length ?? 0) + 1}`);
    if (next !== project) {
      commit(next);
      setPicked([]);
      setSelected(null);
    } else setNotice(t("groupNeedsTwo"));
  }, [project, selected, picked, commit, t]);

  const play = useCallback((r: number) => {
    setRate(r);
    setPlaying(true);
  }, []);

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
        setRate(1);
        setPlaying((p) => !p);
      } else if (!mod && e.code === "KeyK") {
        setPlaying(false);
      } else if (!mod && e.code === "KeyL") {
        // Each press goes faster: 1×, 2×, 4×.
        play(playing && rate > 0 ? Math.min(4, rate * 2) : 1);
      } else if (!mod && e.code === "KeyJ") {
        play(playing && rate < 0 ? Math.max(-4, rate * 2) : -1);
      } else if (!mod && e.code === "KeyM") {
        commit(addMarker(project, frame));
      } else if (!mod && e.code === "KeyF" && selClip) {
        commit(freezeFrame(project, selClip.id, frame));
      } else if (e.code === "ArrowUp" || e.code === "ArrowDown") {
        e.preventDefault();
        setFrame(nextStop(project, frame, e.code === "ArrowDown"));
      } else if (mod && e.code === "KeyC" && selClip) {
        setClipboard(selClip);
        setNotice(t("copied"));
      } else if (mod && e.code === "KeyV" && clipboard) {
        e.preventDefault();
        commit(paste(project, clipboard, frame));
      } else if (mod && e.code === "KeyD" && selClip) {
        e.preventDefault();
        commit(duplicate(project, selClip.id));
      } else if (mod && e.code === "KeyG") {
        e.preventDefault();
        if (e.shiftKey && selClip?.media === "compound") commit(ungroup(project, selClip.id));
        else groupPicked();
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
        setPicked([]);
      } else if (!mod && /^Digit[1-9]$/.test(e.code) && project.angles?.length) {
        // Live multi-camera cutting: the number picks the camera.
        commit(switchAngle(project, frame, +e.code.slice(5) - 1));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project, frame, selClip, exporting, commit, undo, redo, saveProject, deleteSelected, quickText, playing, rate, play, clipboard, groupPicked, t]);

  if (!project) {
    return (
      <>
        <UpdateBanner />
        <Home
          onNew={(w, h) => start(newProject(t("untitled"), w, h), null)}
          onOpen={openPath}
          onImport={(p) =>
            api
              .importTimeline(p)
              .then((proj) => start(proj, null))
              .catch((e) => setNotice(`${t("openFailed")}: ${e}`))
          }
          onRecover={(p) => start(p, null)}
          onTemplate={(p) => start({ ...p, name: t("untitled") }, null)}
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
        <button onClick={() => setTemplateName(project.name)}>{t("saveTemplate")}</button>
        <button onClick={toggleLanguage}>{t("language")}</button>
        <button
          disabled={total === 0}
          onClick={() => {
            setPlaying(false);
            setPublishing(true);
          }}
        >
          {t("publish")}
        </button>
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
          {(["media", "text", "captions", "auto", "fonts", "assistant"] as const).map((k) => (
            <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
              {t(k)}
            </button>
          ))}
        </nav>
        <div className="side-scroll">
          {tab === "media" && (
            <>
              <div className="panel-body record-row">
                <VoiceRecorder project={project} frame={frame} onCommit={commit} onNotice={setNotice} />
              </div>
              <MediaPanel items={media} onImport={importPaths} onAdd={addMedia} />
            </>
          )}
          {tab === "auto" && (
            <>
              <AutoEditPanel font={[brand.font ?? "", "Lalezar", "Cairo"].find((f) => families.includes(f)) ?? families[0] ?? "Cairo"} onOpen={(p) => start(p, null)} onNotice={setNotice} />
              <div className="panel-body">
                <Section title={t("beatSync")}>
                  <BeatSection project={project} onCommit={commit} />
                </Section>
                <Section title={t("followSubject")}>
                  <ReframeSection project={project} onOpen={(p) => start(p, null)} />
                </Section>
              </div>
            </>
          )}
          {tab === "text" && (
            <>
              <TextPresets families={families} brandFont={brand.font} onAdd={addText} />
              <ShapesPanel project={project} frame={frame} onCommit={commit} onSelect={setSelected} />
              <BrandPanel brand={brand} families={families} onChange={setBrand} onAddLogo={addLogo} />
            </>
          )}
          {tab === "assistant" && (
            <AssistantPanel project={project} selected={selClip} missing={missing} brand={brand} families={families} onCommit={commit} onJump={setFrame} media={media} frame={frame} />
          )}
          {tab === "captions" && <CaptionsPanel project={project} families={families} onAdd={commit} />}
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
          rate={rate}
          onFrame={setFrame}
          onStop={() => setPlaying(false)}
        />
        {showScopes && <Scopes frame={frame} revision={revision} playing={playing} />}
        <div className="transport">
          <button onClick={() => setFrame(0)} title="Home">
            {t("toStart")}
          </button>
          <button
            className="play"
            onClick={() => {
              setRate(1);
              setPlaying((p) => !p);
            }} disabled={total === 0} title={t("playPause")} aria-label={t("playPause")}>
            {playing ? "❚❚" : "▶"}
          </button>
          <span className="time" dir="ltr">
            {formatTime(frame, project.fps)} / {formatTime(total, project.fps)}
          </span>
          <div className="spacer" />
          {!!project.angles?.length && <AngleButtons project={project} frame={frame} onCommit={commit} compact />}
          <button className={showScopes ? "on" : ""} onClick={() => setShowScopes((v) => !v)} title={t("scopes")}>
            {t("scopes")}
          </button>
          <button onClick={() => commit(splitAt(project, frame, selClip?.id))} title="S">
            {t("split")}
          </button>
          <button onClick={() => commit(addMarker(project, frame))} title="M">
            {t("marker")}
          </button>
          <button onClick={quickText} title="T">
            {t("addText")}
          </button>
          {playing && rate !== 1 && (
            <span className="rate" dir="ltr">
              {rate > 0 ? `${rate}×` : `◀ ${-rate}×`}
            </span>
          )}
        </div>
      </main>

      <aside className="side right">
        <h3 className="side-title">{t("properties")}</h3>
        <div className="side-scroll">
          {selText && (
            <TextProperties
              layer={selText}
              fonts={fonts}
              swatches={brand.colors}
              fps={fpsValue(project.fps)}
              onChange={(nt) => commit({ ...project, texts: project.texts.map((x) => (x.id === nt.id ? nt : x)) })}
              onDelete={() => deleteSelected(false)}
            />
          )}
          {selShape && (
            <ShapeProperties
              shape={selShape}
              fps={fpsValue(project.fps)}
              swatches={brand.colors}
              onChange={(ns) => commit({ ...project, shapes: (project.shapes ?? []).map((x) => (x.id === ns.id ? ns : x)) })}
              onDelete={() => deleteSelected(false)}
            />
          )}
          {selClip && picked.length > 0 && (
            <div className="panel-body">
              <p className="hint">
                {picked.length + (picked.includes(selClip.id) ? 0 : 1)} {t("clipsPicked")}
              </p>
              <div className="chips">
                <button onClick={groupPicked}>{t("groupClips")} (Ctrl+G)</button>
                <button className="danger" onClick={() => deleteSelected(true)}>
                  {t("deleteClips")}
                </button>
              </div>
            </div>
          )}
          {selClip && (
            <ClipProperties
              clip={selClip}
              fps={fpsValue(project.fps)}
              frame={frame}
              hasBefore={project.tracks.some((tr) => tr.clips.some((c) => c.id === selClip.id) && tr.clips.some((c) => c.id !== selClip.id && c.media !== "audio" && c.start + c.duration === selClip.start))}
              onChange={(c) => commit(updateClip(project, c))}
              onRipple={() => deleteSelected(true)}
              onLift={() => deleteSelected(false)}
              actions={
                <>
                  <button onClick={() => commit(duplicate(project, selClip.id))} title="Ctrl+D">
                    {t("duplicate")}
                  </button>
                  <button onClick={() => setClipboard(selClip)} title="Ctrl+C">
                    {t("copy")}
                  </button>
                  {selClip.media === "video" && (
                    <button onClick={() => commit(freezeFrame(project, selClip.id, frame))} title="F" disabled={frame < selClip.start || frame >= selClip.start + selClip.duration}>
                      {t("freezeFrame")}
                    </button>
                  )}
                  {selClip.media === "video" && selClip.volume > 0 && !selClip.freeze && (
                    <button onClick={() => commit(detachAudio(project, selClip.id))}>{t("detachAudio")}</button>
                  )}
                  {selClip.media === "compound" && (
                    <button onClick={() => commit(ungroup(project, selClip.id))} title="Ctrl+Shift+G">
                      {t("ungroup")}
                    </button>
                  )}
                  {(() => {
                    const tr = project.tracks.find((x) => x.clips.some((c) => c.id === selClip.id));
                    return tr && <button onClick={() => commit(closeGaps(project, tr.id))}>{t("closeGaps")}</button>;
                  })()}
                </>
              }
            />
          )}
          {!selText && !selClip && !selShape && (
            <div className="panel-body">
              <p className="hint">{total === 0 ? t("emptyHint") : t("selectHint")}</p>
              <p className="hint small">{t("shortcutsHint")}</p>
            </div>
          )}
        </div>
      </aside>

      <section className="timeline-area">
        <Timeline project={project} frame={frame} selected={selected} onSeek={setFrame} onSelect={setSelected} onCommit={commit} picked={picked} onPick={setPicked} />
      </section>

      {publishing && <PublishDialog project={project} onClose={() => setPublishing(false)} />}
      {exporting && <ExportDialog project={project} onClose={() => setExporting(false)} onLoudness={(v) => commit({ ...project, loudness: v })} />}
      {templateName !== null && (
        <div className="modal-back">
          <form
            className="modal"
            onSubmit={(e) => {
              e.preventDefault();
              if (!templateName.trim()) return;
              api
                .saveTemplate(project, templateName.trim())
                .then(() => setNotice(t("templateSaved")))
                .catch((err) => setNotice(String(err)));
              setTemplateName(null);
            }}
          >
            <h2>{t("saveTemplate")}</h2>
            <label>
              {t("templateName")}
              <input autoFocus dir="auto" value={templateName} onChange={(e) => setTemplateName(e.target.value)} />
            </label>
            <div className="modal-actions">
              <button className="primary" type="submit">
                {t("ok")}
              </button>
              <button type="button" onClick={() => setTemplateName(null)}>
                {t("cancel")}
              </button>
            </div>
          </form>
        </div>
      )}
      {notice && <div className="toast">{notice}</div>}
    </div>
  );
}
