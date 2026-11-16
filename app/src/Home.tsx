import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type TemplateInfo } from "./api";
import { toggleLanguage, useT } from "./i18n";
import type { Project, Recent } from "./types";
import { FORMATS, allClips, fileName } from "./types";

interface Props {
  onNew: (width: number, height: number) => void;
  onOpen: (path: string) => void;
  onRecover: (p: Project) => void;
  onTemplate: (p: Project) => void;
}

/** First screen: start something new, reopen recent work, or recover. */
export function Home({ onNew, onOpen, onRecover, onTemplate }: Props) {
  const t = useT();
  const [recent, setRecent] = useState<Recent[]>([]);
  const [recovered, setRecovered] = useState<Project | null>(null);
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);

  useEffect(() => {
    api.recent().then(setRecent).catch(() => {});
    api.templates().then(setTemplates).catch(() => {});
    api
      .recover()
      .then((p) => p && allClips(p).length + p.texts.length > 0 && setRecovered(p))
      .catch(() => {});
  }, []);

  const pick = async () => {
    const path = await openDialog({ filters: [{ name: "AVE", extensions: ["ave"] }] });
    if (typeof path === "string") onOpen(path);
  };

  return (
    <div className="home">
      <header className="home-head">
        <h1>{t("app")}</h1>
        <button onClick={toggleLanguage}>{t("language")}</button>
      </header>

      {recovered && (
        <div className="banner">
          <span>
            {t("recoverFound")}: <b dir="auto">{recovered.name}</b>
          </span>
          <button className="primary" onClick={() => onRecover(recovered)}>
            {t("recover")}
          </button>
          <button
            onClick={() => {
              api.discardAutosave();
              setRecovered(null);
            }}
          >
            {t("discard")}
          </button>
        </div>
      )}

      <section>
        <h2>{t("newProject")}</h2>
        <p className="muted">{t("chooseFormat")}</p>
        <div className="formats">
          {FORMATS.map((f) => (
            <button key={f.id} className="format" onClick={() => onNew(f.width, f.height)}>
              <span className="shape" style={{ aspectRatio: `${f.width} / ${f.height}` }} />
              <b>{t(f.label)}</b>
              <span className="muted" dir="ltr">
                {f.id} · {f.width}×{f.height}
              </span>
            </button>
          ))}
        </div>
      </section>

      <section>
        <h2>{t("templates")}</h2>
        {templates.length === 0 && <p className="muted">{t("noTemplates")}</p>}
        <div className="formats">
          {templates.map((tp) => (
            <div key={tp.path} className="template">
              <button className="format" onClick={() => api.loadTemplate(tp.path).then(onTemplate)}>
                <span className="shape" style={{ aspectRatio: `${tp.width} / ${tp.height}` }} />
                <b dir="auto">{tp.name}</b>
                <span className="muted">
                  {tp.texts} {t("textsCount")}
                </span>
              </button>
              <button
                className="icon remove"
                title={t("deleteTemplate")}
                aria-label={t("deleteTemplate")}
                onClick={() => api.deleteTemplate(tp.path).then(() => setTemplates((x) => x.filter((y) => y.path !== tp.path)))}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      </section>

      <section>
        <div className="section-head">
          <h2>{t("recent")}</h2>
          <button onClick={pick}>{t("openProject")}</button>
        </div>
        {recent.length === 0 && <p className="muted">{t("noRecent")}</p>}
        <ul className="recent">
          {recent.map((r) => (
            <li key={r.path}>
              <button className="recent-open" onClick={() => onOpen(r.path)}>
                <b dir="auto">{r.name || fileName(r.path)}</b>
                <span className="muted" dir="ltr">
                  {r.path}
                </span>
                <span className="muted">{new Date(r.opened * 1000).toLocaleString()}</span>
              </button>
              <button
                className="icon"
                title={t("forget")}
                aria-label={t("forget")}
                onClick={() => api.forgetRecent(r.path).then(() => setRecent((x) => x.filter((y) => y.path !== r.path)))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
