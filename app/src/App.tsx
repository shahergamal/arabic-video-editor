import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { useHistory } from "./history";
import { appendClip, rippleDelete, splitAt } from "./edits";
import { Preview } from "./Preview";
import { Timeline } from "./Timeline";
import { TextPanel } from "./TextPanel";
import type { Project, TextLayer } from "./types";
import { FORMATS, fpsValue, newId, projectDuration } from "./types";

const VIDEO_TYPES = ["mp4", "mov", "mkv", "webm", "m4v", "avi", "mts"];

export function App() {
  const { present: project, commit, reset, undo, redo } = useHistory<Project>(null);
  const [revision, setRevision] = useState(0);
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [fonts, setFonts] = useState<string[]>([]);
  const [recovered, setRecovered] = useState<Project | null>(null);
  const [exporting, setExporting] = useState<{ done: number; total: number } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    api.fonts().then(setFonts).catch(() => {});
    api.recover().then((p) => p && p.clips.length + p.texts.length > 0 && setRecovered(p)).catch(() => {});
    const un1 = listen<{ done: number; total: number }>("export-progress", (e) => setExporting(e.payload));
    const un2 = listen<{ ok: boolean; message: string }>("export-finished", (e) => {
      setExporting(null);
      setNotice(e.payload.ok ? `تم التصدير: ${e.payload.message}` : `لم يكتمل التصدير: ${e.payload.message}`);
    });
    return () => {
      un1.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  // Every change goes to the engine (and its autosave) before the preview redraws.
  useEffect(() => {
    if (!project) return;
    api
      .update(project)
      .then(() => setRevision((r) => r + 1))
      .catch((e) => setNotice(`تعذّر الحفظ التلقائي: ${e}`));
  }, [project]);

  const importVideo = useCallback(async () => {
    if (!project) return;
    const picked = await openDialog({ multiple: true, filters: [{ name: "فيديو", extensions: VIDEO_TYPES }] });
    if (!picked) return;
    let next = project;
    for (const path of Array.isArray(picked) ? picked : [picked]) {
      try {
        const info = await api.probe(path);
        next = appendClip(next, path, info.duration);
      } catch (e) {
        setNotice(`لم نتمكن من فتح الملف: ${e}`);
      }
    }
    commit(next);
  }, [project, commit]);

  const addText = useCallback(() => {
    if (!project) return;
    const layer: TextLayer = {
      id: newId(),
      text: "اكتب هنا",
      font_family: fonts.includes("Cairo") ? "Cairo" : fonts[0] ?? "IBM Plex Sans Arabic",
      size: 0.06,
      weight: 700,
      color: [255, 255, 255, 255],
      x: 0.5,
      y: 0.8,
      box_width: 0.9,
      shadow: true,
      start: frame,
      duration: Math.round(fpsValue(project.fps) * 3),
    };
    commit({ ...project, texts: [...project.texts, layer] });
    setSelected(layer.id);
  }, [project, fonts, frame, commit]);

  const doExport = useCallback(async () => {
    if (!project || projectDuration(project) === 0) return;
    const path = await saveDialog({ defaultPath: `${project.name}.mp4`, filters: [{ name: "MP4", extensions: ["mp4"] }] });
    if (!path) return;
    setPlaying(false);
    setExporting({ done: 0, total: projectDuration(project) });
    api.startExport(path).catch((e) => {
      setExporting(null);
      setNotice(String(e));
    });
  }, [project]);

  const saveProject = useCallback(async () => {
    if (!project) return;
    const path = await saveDialog({ defaultPath: `${project.name}.ave`, filters: [{ name: "مشروع", extensions: ["ave"] }] });
    if (path) await api.save(project, path).then(() => setNotice("تم حفظ المشروع"));
  }, [project]);

  const openProject = useCallback(async () => {
    const path = await openDialog({ filters: [{ name: "مشروع", extensions: ["ave"] }] });
    if (typeof path === "string") reset(await api.open(path));
  }, [reset]);

  // Keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement).closest("input, textarea, select");
      if (typing || !project) return;
      const mod = e.ctrlKey || e.metaKey;
      if (e.code === "Space") {
        e.preventDefault();
        setPlaying((p) => !p);
      } else if (mod && e.code === "KeyZ") {
        e.preventDefault();
        e.shiftKey ? redo() : undo();
      } else if (mod && e.code === "KeyS") {
        e.preventDefault();
        saveProject();
      } else if (e.code === "KeyS") {
        commit(splitAt(project, frame));
      } else if ((e.code === "Delete" || e.code === "Backspace") && selected) {
        if (project.clips.some((c) => c.id === selected)) commit(rippleDelete(project, selected));
        else commit({ ...project, texts: project.texts.filter((t) => t.id !== selected) });
        setSelected(null);
      } else if (e.code === "ArrowRight" || e.code === "ArrowLeft") {
        const step = e.shiftKey ? Math.round(fpsValue(project.fps)) : 1;
        setFrame((f) => Math.max(0, f + (e.code === "ArrowRight" ? step : -step)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project, frame, selected, commit, undo, redo, saveProject]);

  if (!project) {
    return (
      <StartScreen
        recovered={recovered}
        onRecover={() => recovered && reset(recovered)}
        onOpen={openProject}
        onNew={(w, h, label) => reset({ version: 1, name: `مشروع ${label}`, width: w, height: h, fps: { num: 30, den: 1 }, clips: [], texts: [] })}
      />
    );
  }

  const text = project.texts.find((t) => t.id === selected);
  const clip = project.clips.find((c) => c.id === selected);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">محرر الفيديو</div>
        <input className="project-name" value={project.name} onChange={(e) => commit({ ...project, name: e.target.value })} />
        <div className="spacer" />
        <button onClick={undo} title="تراجع (Ctrl+Z)">تراجع</button>
        <button onClick={redo} title="إعادة (Ctrl+Shift+Z)">إعادة</button>
        <button onClick={saveProject} title="حفظ (Ctrl+S)">حفظ</button>
        <button className="primary" onClick={doExport} disabled={!!exporting || projectDuration(project) === 0}>
          تصدير
        </button>
      </header>

      <main className="workspace">
        <section className="panel media">
          <h2>الوسائط</h2>
          <div className="panel-body">
            <button className="wide" onClick={importVideo}>+ استيراد فيديو</button>
            {project.clips.length === 0 && <p className="hint">ابدأ باستيراد فيديو. الملف الأصلي لا يتغير أبداً.</p>}
            <ul className="media-list">
              {[...new Set(project.clips.map((c) => c.source))].map((s) => (
                <li key={s} dir="auto" title={s}>{s.split(/[\\/]/).pop()}</li>
              ))}
            </ul>
          </div>
        </section>

        <section className="stage">
          <Preview
            project={project}
            revision={revision}
            frame={frame}
            playing={playing}
            onFrame={setFrame}
            onStop={() => setPlaying(false)}
          />
          <div className="transport">
            <button onClick={() => setFrame(0)} title="إلى البداية">⏮</button>
            <button className="play" onClick={() => setPlaying((p) => !p)} title="تشغيل / إيقاف (مسافة)">
              {playing ? "❚❚" : "▶"}
            </button>
            <button onClick={() => commit(splitAt(project, frame))} title="قص عند المؤشر (S)">قص</button>
            <button onClick={addText} title="نص جديد عند المؤشر">+ نص</button>
          </div>
        </section>

        <section className="panel props">
          <h2>الخصائص</h2>
          {text ? (
            <TextPanel
              layer={text}
              fonts={fonts}
              fps={fpsValue(project.fps)}
              onChange={(t) => commit({ ...project, texts: project.texts.map((x) => (x.id === t.id ? t : x)) })}
              onDelete={() => {
                commit({ ...project, texts: project.texts.filter((x) => x.id !== text.id) });
                setSelected(null);
              }}
            />
          ) : clip ? (
            <div className="panel-body">
              <p dir="auto" className="filename">{clip.source.split(/[\\/]/).pop()}</p>
              <label>
                مستوى الصوت
                <input
                  type="range" min={0} max={2} step={0.05} value={clip.volume}
                  onChange={(e) => commit({ ...project, clips: project.clips.map((c) => (c.id === clip.id ? { ...c, volume: +e.target.value } : c)) })}
                />
              </label>
              <button className="danger" onClick={() => { commit(rippleDelete(project, clip.id)); setSelected(null); }}>
                حذف المقطع وسحب الفراغ
              </button>
            </div>
          ) : (
            <p className="hint panel-body">اختر مقطعاً أو نصاً من الخط الزمني لتعديله.</p>
          )}
        </section>
      </main>

      <Timeline project={project} frame={frame} selected={selected} onSeek={(f) => { setPlaying(false); setFrame(f); }} onSelect={setSelected} />

      {exporting && (
        <div className="overlay">
          <div className="dialog">
            <h3>جارٍ التصدير</h3>
            <progress max={exporting.total} value={exporting.done} />
            <p>{Math.round((exporting.done / Math.max(1, exporting.total)) * 100)}٪</p>
            <button onClick={() => api.cancelExport()}>إلغاء</button>
          </div>
        </div>
      )}
      {notice && (
        <div className="toast" onClick={() => setNotice(null)} dir="auto">
          {notice}
        </div>
      )}
    </div>
  );
}

function StartScreen(props: {
  recovered: Project | null;
  onRecover: () => void;
  onOpen: () => void;
  onNew: (w: number, h: number, label: string) => void;
}) {
  return (
    <div className="start">
      <h1>محرر الفيديو</h1>
      {props.recovered && (
        <div className="recover">
          <span>وجدنا شغلك من آخر مرة: «{props.recovered.name}»</span>
          <button className="primary" onClick={props.onRecover}>استرجاع</button>
        </div>
      )}
      <h2>مشروع جديد</h2>
      <div className="formats">
        {FORMATS.map((f) => (
          <button key={f.id} className="format" onClick={() => props.onNew(f.width, f.height, f.label)}>
            <span className="shape" style={{ aspectRatio: `${f.width} / ${f.height}` }} />
            <strong>{f.label}</strong>
            <span className="muted">{f.id}</span>
          </button>
        ))}
      </div>
      <button onClick={props.onOpen}>فتح مشروع محفوظ</button>
    </div>
  );
}
