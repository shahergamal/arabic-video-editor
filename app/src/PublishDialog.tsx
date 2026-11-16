// Publishing: export the right shapes, then either upload straight to
// Facebook, Instagram and TikTok (through the user's own developer apps)
// or copy the caption and open the site to post by hand. Nothing is
// uploaded until the user presses Publish.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, type ExportJob } from "./api";
import { api5, type Outcome, type Platform, type PostKind, type SocialView, type Target } from "./api5";
import { useT, type TKey } from "./i18n";
import type { Project } from "./types";
import { fpsValue, projectDuration } from "./types";

export const KINDS: Record<Platform, PostKind[]> = {
  facebook: ["post", "reel", "story"],
  instagram: ["post", "reel", "story"],
  tiktok: ["post"],
};
const LIMIT: Record<Platform, number> = { facebook: 63206, instagram: 2200, tiktok: 2200 };
const STORY_MAX = 60;
const PLATFORMS: Platform[] = ["facebook", "instagram", "tiktok"];

const tall = (p: { width: number; height: number }) => Math.abs(p.width * 16 - p.height * 9) <= Math.max(p.width, p.height) * 0.02;
const even = (v: number) => Math.max(2, Math.round(v / 2) * 2);
const key = (t: Target) => `${t.platform}:${t.kind}`;

/** The caption with the hashtags added once each, in order. */
export function fullCaption(caption: string, tags: string): string {
  const seen = new Set<string>();
  const list = tags
    .split(/[\s,،]+/)
    .map((x) => x.trim().replace(/^#+/, ""))
    .filter((x) => x && !seen.has(x.toLowerCase()) && seen.add(x.toLowerCase()) && !caption.includes(`#${x}`))
    .map((x) => `#${x}`);
  return [caption.trim(), list.join(" ")].filter(Boolean).join("\n\n");
}

/** Problems that stop a target from going out as it is. */
export function targetProblems(project: Project, targets: Target[], caption: string, hasVertical: boolean): { target: Target; problem: TKey }[] {
  const seconds = projectDuration(project) / fpsValue(project.fps);
  const out: { target: Target; problem: TKey }[] = [];
  for (const t of targets) {
    if (caption.length > LIMIT[t.platform]) out.push({ target: t, problem: "captionTooLong" });
    if (t.kind === "story" && seconds > STORY_MAX) out.push({ target: t, problem: "storyTooLong" });
    if ((t.kind === "reel" || t.kind === "story" || t.platform === "tiktok") && !tall(project) && !hasVertical) out.push({ target: t, problem: "needsTall" });
  }
  return out;
}

type Step = { s: "setup" } | { s: "exporting"; done: number; total: number } | { s: "uploading"; done: number; total: number } | { s: "done"; outcomes: Outcome[] | null; files: string[]; manual: boolean } | { s: "failed"; message: string };

export function PublishDialog({ project, onClose }: { project: Project; onClose: () => void }) {
  const t = useT();
  const [social, setSocial] = useState<SocialView | null>(null);
  const [chosen, setChosen] = useState<Target[]>([]);
  const [pageId, setPageId] = useState("");
  const [caption, setCaption] = useState("");
  const [tags, setTags] = useState("");
  const [title, setTitle] = useState("");
  const [privacy, setPrivacy] = useState("SELF_ONLY");
  const [privacyOptions, setPrivacyOptions] = useState<string[]>([]);
  const [comments, setComments] = useState(true);
  const [makeTall, setMakeTall] = useState(true);
  const [direct, setDirect] = useState(true);
  const [showSetup, setShowSetup] = useState(false);
  const [step, setStep] = useState<Step>({ s: "setup" });
  const [files, setFiles] = useState<string[]>([]);

  useEffect(() => {
    api5.socialStatus().then((v) => {
      setSocial(v);
      setPageId((p) => p || v.pages[0]?.id || "");
    });
    const a = listen<{ ok: boolean; message: string; result: Outcome[] | null }>("publish-finished", (e) => setStep({ s: "done", outcomes: e.payload.result, files: [], manual: false }));
    const b = listen<{ done: number; total: number }>("publish-progress", (e) => setStep({ s: "uploading", ...e.payload }));
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);
  useEffect(() => {
    if (social?.tiktok_name)
      api5
        .tiktokPrivacy()
        .then(setPrivacyOptions)
        .catch(() => setPrivacyOptions([]));
  }, [social?.tiktok_name]);

  const text = fullCaption(caption, tags);
  const connected = (p: Platform) => (p === "tiktok" ? !!social?.tiktok_name : !!social?.meta_name && (p === "facebook" ? social.pages.length > 0 : social.pages.some((x) => x.id === pageId && x.ig_username)));
  const allConnected = chosen.length > 0 && chosen.every((c) => connected(c.platform));
  const useDirect = direct && allConnected;
  const needVertical = makeTall && !tall(project) && chosen.some((c) => c.kind !== "post" || c.platform === "tiktok");
  const problems = targetProblems(project, chosen, text, needVertical);
  const toggle = (platform: Platform, kind: PostKind) => {
    const tgt = { platform, kind, page_id: pageId };
    setChosen((c) => (c.some((x) => key(x) === key(tgt)) ? c.filter((x) => key(x) !== key(tgt)) : [...c, tgt]));
  };

  const go = async () => {
    const path = await saveDialog({ defaultPath: `${project.name || "video"}.mp4`, filters: [{ name: "MP4", extensions: ["mp4"] }] });
    if (!path) return;
    const jobs: ExportJob[] = [{ path, width: even(project.width), height: even(project.height), crf: 20, encoder: "auto" }];
    const vertical = needVertical ? path.replace(/\.mp4$/i, "") + " 9x16.mp4" : null;
    if (vertical) {
      const long = Math.max(project.width, project.height);
      jobs.push({ path: vertical, width: even((long * 9) / 16), height: even(long), crf: 20, fill: true, encoder: "auto" });
    }
    setStep({ s: "exporting", done: 0, total: 1 });
    const offProgress = await listen<{ done: number; total: number; job: number; jobs: number }>("export-progress", (e) =>
      setStep({ s: "exporting", done: e.payload.job * e.payload.total + e.payload.done, total: e.payload.jobs * Math.max(1, e.payload.total) }),
    );
    const offDone = await listen<{ ok: boolean; message: string }>("export-finished", async (e) => {
      offProgress();
      offDone();
      if (!e.payload.ok) {
        setStep({ s: "failed", message: e.payload.message });
        return;
      }
      const files = vertical ? [path, vertical] : [path];
      if (!useDirect) {
        // By hand: the caption goes to the clipboard and the sites open.
        await navigator.clipboard.writeText(text).catch(() => {});
        for (const p of [...new Set(chosen.map((c) => c.platform))]) await api5.openPlatform(p).catch(() => {});
        setStep({ s: "done", outcomes: null, files, manual: true });
        return;
      }
      setFiles(files);
      setStep({ s: "uploading", done: 0, total: chosen.length * 100 });
      api5
        .publish({
          file: path,
          vertical_file: vertical,
          caption: text,
          title,
          targets: chosen.map((c) => ({ ...c, page_id: pageId })),
          tiktok_privacy: privacy,
          tiktok_allow_comments: comments,
        })
        .catch((err) => setStep({ s: "failed", message: String(err) }));
    });
    api.startExport(jobs).catch((err) => setStep({ s: "failed", message: String(err) }));
  };

  return (
    <div className="modal-back">
      <div className="modal publish" role="dialog" aria-label={t("publishTitle")}>
        <h2>{t("publishTitle")}</h2>
        {step.s === "setup" && (
          <>
            <p className="label">{t("whereTo")}</p>
            <div className="publish-grid">
              {PLATFORMS.map((p) => (
                <div key={p} className="publish-platform">
                  <b>
                    {t(`pf_${p}` as TKey)} {connected(p) && <span className="ok" title={t("connected")}>●</span>}
                  </b>
                  <div className="chips">
                    {KINDS[p].map((k) => (
                      <button key={k} className={chosen.some((c) => c.platform === p && c.kind === k) ? "on" : ""} onClick={() => toggle(p, k)}>
                        {t(`kind_${k}` as TKey)}
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>
            {social && social.pages.length > 1 && chosen.some((c) => c.platform !== "tiktok") && (
              <label>
                {t("page")}
                <select value={pageId} onChange={(e) => setPageId(e.target.value)}>
                  {social.pages.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                      {p.ig_username ? ` · @${p.ig_username}` : ""}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <label>
              {t("caption")}
              <textarea dir="auto" rows={4} value={caption} onChange={(e) => setCaption(e.target.value)} placeholder={t("captionPlaceholder")} />
            </label>
            <label>
              {t("hashtags")}
              <input dir="auto" value={tags} onChange={(e) => setTags(e.target.value)} placeholder="#عقارات #القاهرة" />
            </label>
            {chosen.some((c) => c.platform === "facebook") && (
              <label>
                {t("videoTitle")}
                <input dir="auto" value={title} onChange={(e) => setTitle(e.target.value)} />
              </label>
            )}
            <p className="hint small" dir="auto">
              {text.length} {t("characters")}
            </p>
            {!tall(project) && chosen.some((c) => c.kind !== "post" || c.platform === "tiktok") && (
              <label className="check">
                <input type="checkbox" checked={makeTall} onChange={(e) => setMakeTall(e.target.checked)} />
                {t("makeTallCopy")}
              </label>
            )}
            {chosen.some((c) => c.platform === "tiktok") && connected("tiktok") && (
              <>
                <label>
                  {t("tiktokPrivacy")}
                  <select value={privacy} onChange={(e) => setPrivacy(e.target.value)}>
                    {(privacyOptions.length ? privacyOptions : ["SELF_ONLY"]).map((o) => (
                      <option key={o} value={o}>
                        {t(`privacy_${o}` as TKey) ?? o}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="check">
                  <input type="checkbox" checked={comments} onChange={(e) => setComments(e.target.checked)} />
                  {t("allowComments")}
                </label>
                <p className="hint small">{t("tiktokPrivateHint")}</p>
              </>
            )}
            {problems.map((p, i) => (
              <p key={i} className="hint small error">
                {t(`pf_${p.target.platform}` as TKey)} {t(`kind_${p.target.kind}` as TKey)}: {t(p.problem)}
              </p>
            ))}
            {chosen.length > 0 && (
              <div className="publish-mode">
                {allConnected ? (
                  <label className="check">
                    <input type="checkbox" checked={direct} onChange={(e) => setDirect(e.target.checked)} />
                    {t("publishDirect")}
                  </label>
                ) : (
                  <p className="hint small">{t("publishManualHint")}</p>
                )}
              </div>
            )}
            <button className="link" onClick={() => setShowSetup((v) => !v)}>
              {t("connectAccounts")} {showSetup ? "▲" : "▼"}
            </button>
            {showSetup && social && <AccountsSetup social={social} onChange={setSocial} />}
            <div className="modal-actions">
              <button className="primary" disabled={!chosen.length || problems.length > 0} onClick={go}>
                {useDirect ? t("publishNow") : t("exportAndOpen")}
              </button>
              <button onClick={onClose}>{t("cancel")}</button>
            </div>
            {useDirect && <p className="hint small">{t("uploadConsent")}</p>}
          </>
        )}
        {(step.s === "exporting" || step.s === "uploading") && (
          <>
            <p>{step.s === "exporting" ? t("exporting") : t("uploading")}…</p>
            <progress value={step.done} max={Math.max(1, step.total)} />
            <div className="modal-actions">
              <button onClick={() => api.cancelExport()}>{t("cancel")}</button>
            </div>
          </>
        )}
        {step.s === "failed" && (
          <>
            <p className="error">{t("publishFailed")}</p>
            <p className="muted" dir="auto">
              {step.message}
            </p>
            <div className="modal-actions">
              <button onClick={() => setStep({ s: "setup" })}>{t("back")}</button>
              <button onClick={onClose}>{t("close")}</button>
            </div>
          </>
        )}
        {step.s === "done" && (
          <>
            {step.manual ? (
              <>
                <p className="ok">{t("manualReady")}</p>
                <ol className="steps">
                  <li>{t("manualStep1")}</li>
                  <li>{t("manualStep2")}</li>
                  <li>{t("manualStep3")}</li>
                </ol>
                <p className="muted export-files" dir="auto">
                  {(step.files.length ? step.files : files).join("\n")}
                </p>
                <button onClick={() => navigator.clipboard.writeText(text)}>{t("copyCaptionAgain")}</button>
              </>
            ) : (
              <ul className="outcomes">
                {(step.outcomes ?? []).map((o, i) => (
                  <li key={i} className={o.ok ? "ok" : "error"}>
                    {o.ok ? "✓" : "✕"} {t(`pf_${o.target.platform}` as TKey)} {t(`kind_${o.target.kind}` as TKey)}
                    {!o.ok && (
                      <span className="muted" dir="auto">
                        {" "}
                        {o.message}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            )}
            <div className="modal-actions">
              <button className="primary" onClick={onClose}>
                {t("close")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/** Keys of the user's own Meta and TikTok developer apps, and signing in. */
function AccountsSetup({ social, onChange }: { social: SocialView; onChange: (s: SocialView) => void }) {
  const t = useT();
  const [metaId, setMetaId] = useState("");
  const [metaSecret, setMetaSecret] = useState("");
  const [ttKey, setTtKey] = useState("");
  const [ttSecret, setTtSecret] = useState("");
  const [waiting, setWaiting] = useState<"meta" | "tiktok" | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => {
    const un = listen<{ ok: boolean; message: string; result: SocialView | null }>("social-finished", (e) => {
      setWaiting(null);
      if (e.payload.result) onChange(e.payload.result);
      setMsg(e.payload.ok ? t("connectedOk") : e.payload.message);
    });
    return () => {
      un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const connect = (p: "meta" | "tiktok") => {
    setMsg(null);
    setWaiting(p);
    api5.socialConnect(p).catch((e) => {
      setWaiting(null);
      setMsg(String(e));
    });
  };
  const saveApp = (p: "meta" | "tiktok", id: string, secret: string) =>
    api5
      .socialSetApps(p, id, secret)
      .then(onChange)
      .catch((e) => setMsg(String(e)));
  return (
    <div className="accounts">
      <p className="hint small">{t("accountsHint")}</p>
      <p className="hint small">
        {t("redirectToRegister")}: <code dir="ltr">{social.redirect}</code>
      </p>

      <h4>{t("metaApp")}</h4>
      {social.meta_name ? (
        <p>
          ✓ {social.meta_name} · {social.pages.length} {t("pages")}{" "}
          <button onClick={() => api5.socialDisconnect("meta").then(onChange)}>{t("disconnect")}</button>
        </p>
      ) : (
        <>
          <div className="row">
            <input dir="ltr" placeholder={social.meta_app ? "App ID ✓" : "App ID"} value={metaId} onChange={(e) => setMetaId(e.target.value)} />
            <input dir="ltr" type="password" placeholder={social.meta_app ? "App secret ✓" : "App secret"} value={metaSecret} onChange={(e) => setMetaSecret(e.target.value)} />
          </div>
          <div className="row">
            {(metaId || metaSecret) && <button onClick={() => saveApp("meta", metaId, metaSecret).then(() => (setMetaId(""), setMetaSecret("")))}>{t("save")}</button>}
            <button disabled={!social.meta_app || waiting !== null} onClick={() => connect("meta")}>
              {waiting === "meta" ? `${t("waitingBrowser")}…` : t("connectFacebook")}
            </button>
            <button className="link" onClick={() => api5.openPlatform("meta-dev")}>
              {t("openDevSite")}
            </button>
          </div>
        </>
      )}
      {social.meta_name && social.pages.length > 0 && !social.pages.some((p) => p.ig_username) && <p className="hint small">{t("noInstagramLinked")}</p>}

      <h4>{t("tiktokApp")}</h4>
      {social.tiktok_name ? (
        <p>
          ✓ {social.tiktok_name} <button onClick={() => api5.socialDisconnect("tiktok").then(onChange)}>{t("disconnect")}</button>
        </p>
      ) : (
        <>
          <div className="row">
            <input dir="ltr" placeholder={social.tiktok_app ? "Client key ✓" : "Client key"} value={ttKey} onChange={(e) => setTtKey(e.target.value)} />
            <input dir="ltr" type="password" placeholder={social.tiktok_app ? "Client secret ✓" : "Client secret"} value={ttSecret} onChange={(e) => setTtSecret(e.target.value)} />
          </div>
          <div className="row">
            {(ttKey || ttSecret) && <button onClick={() => saveApp("tiktok", ttKey, ttSecret).then(() => (setTtKey(""), setTtSecret("")))}>{t("save")}</button>}
            <button disabled={!social.tiktok_app || waiting !== null} onClick={() => connect("tiktok")}>
              {waiting === "tiktok" ? `${t("waitingBrowser")}…` : t("connectTiktok")}
            </button>
            <button className="link" onClick={() => api5.openPlatform("tiktok-dev")}>
              {t("openDevSite")}
            </button>
          </div>
        </>
      )}
      {msg && <p className="hint small">{msg}</p>}
    </div>
  );
}
