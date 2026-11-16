//! Publishing a finished video to Facebook, Instagram and TikTok.
//!
//! Everything goes through the user's OWN developer apps (Meta and
//! TikTok), signed in with their own accounts: nothing passes through a
//! server of ours. A video is uploaded only when the user presses Publish.
//! Keys and sign-ins are kept in `social.json` in the app's data folder on
//! this computer.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::commands::State;

/// Graph API version used for every Meta call.
const GRAPH: &str = "https://graph.facebook.com/v23.0";
const RUPLOAD_IG: &str = "https://rupload.facebook.com/ig-api-upload/v23.0";
const TIKTOK: &str = "https://open.tiktokapis.com/v2";
/// Fixed so it can be registered in the developer apps.
pub const REDIRECT_PORT: u16 = 47321;
pub fn redirect_uri() -> String {
    format!("http://localhost:{REDIRECT_PORT}/callback")
}

const META_SCOPES: &str = "pages_show_list,pages_read_engagement,pages_manage_posts,business_management,instagram_basic,instagram_content_publish";
const TIKTOK_SCOPES: &str = "user.info.basic,video.publish";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub ig_id: String,
    #[serde(default)]
    pub ig_username: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Social {
    #[serde(default)]
    pub meta_app_id: String,
    #[serde(default)]
    pub meta_app_secret: String,
    #[serde(default)]
    pub meta_user_token: String,
    #[serde(default)]
    pub meta_name: String,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub tiktok_client_key: String,
    #[serde(default)]
    pub tiktok_client_secret: String,
    #[serde(default)]
    pub tiktok_access: String,
    #[serde(default)]
    pub tiktok_refresh: String,
    /// Unix seconds when the access token runs out.
    #[serde(default)]
    pub tiktok_expires: u64,
    #[serde(default)]
    pub tiktok_name: String,
}

fn file(data: &Path) -> PathBuf {
    data.join("social.json")
}

pub fn load(data: &Path) -> Social {
    std::fs::read(file(data)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn store(data: &Path, s: &Social) -> Result<(), String> {
    let path = file(data);
    std::fs::write(&path, serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// An HTTP client that hands back error bodies instead of failing on them,
/// so the platform's own explanation reaches the user.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(60 * 30)))
        .build()
        .into()
}

fn json(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<serde_json::Value, String> {
    let mut r = resp.map_err(|e| e.to_string())?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text.clone()));
    if status >= 400 || v.get("error").map(|e| !e.is_null() && e.get("code").and_then(|c| c.as_str()) != Some("ok")).unwrap_or(false) {
        return Err(platform_error(&v).unwrap_or_else(|| format!("HTTP {status}: {}", text.chars().take(300).collect::<String>())));
    }
    Ok(v)
}

/// The readable part of a Meta or TikTok error.
pub fn platform_error(v: &serde_json::Value) -> Option<String> {
    let e = v.get("error")?;
    if let Some(s) = e.as_str() {
        return Some(v.get("error_description").and_then(|d| d.as_str()).map(|d| format!("{s}: {d}")).unwrap_or_else(|| s.to_string()));
    }
    let msg = e.get("error_user_msg").or_else(|| e.get("message")).and_then(|m| m.as_str()).unwrap_or("");
    let code = e.get("code").map(|c| c.to_string()).unwrap_or_default();
    if msg.is_empty() && code == "\"ok\"" {
        return None;
    }
    Some(format!("{msg} ({})", code.trim_matches('"')))
}

fn s(v: &serde_json::Value, path: &[&str]) -> String {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    match cur {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

// ---------- signing in ----------

fn random_text(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).expect("random");
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    b.iter().map(|x| ABC[*x as usize % ABC.len()] as char).collect()
}

/// Waits (at most five minutes) for the browser to come back to our
/// redirect address and returns the `code` it carries.
fn wait_for_code(listener: TcpListener, state: &str) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let until = Instant::now() + Duration::from_secs(300);
    loop {
        if Instant::now() > until {
            return Err("timed out waiting for sign-in".into());
        }
        let (mut sock, _) = match listener.accept() {
            Ok(x) => x,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let _ = sock.set_nonblocking(false);
        let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = [0u8; 8192];
        let n = sock.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let target = req.split_whitespace().nth(1).unwrap_or("");
        if !target.starts_with("/callback") {
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        }
        let query = target.split_once('?').map(|x| x.1).unwrap_or("");
        let params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let get = |k: &str| params.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone());
        let ok = get("state").as_deref() == Some(state) && get("code").is_some();
        let page = if ok {
            "<h2 dir=rtl style='font-family:sans-serif'>تم الربط. ارجع للبرنامج.</h2><p style='font-family:sans-serif'>Connected. You can go back to the app.</p>"
        } else {
            "<h2 dir=rtl style='font-family:sans-serif'>لم يكتمل الربط.</h2><p style='font-family:sans-serif'>Sign-in did not finish.</p>"
        };
        let body = format!("<!doctype html><meta charset=utf-8>{page}");
        let _ = sock.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        if ok {
            return Ok(get("code").unwrap());
        }
        if let Some(e) = get("error_description").or_else(|| get("error")).or_else(|| get("error_message")) {
            return Err(e);
        }
    }
}

/// Opens a sign-in or upload page of one of the three platforms.
pub fn open_url(url: &str) -> Result<(), String> {
    let allowed = ["https://www.facebook.com/", "https://www.instagram.com/", "https://www.tiktok.com/", "https://business.facebook.com/", "https://developers.facebook.com/", "https://developers.tiktok.com/"];
    if !allowed.iter().any(|a| url.starts_with(a)) {
        return Err("address not allowed".into());
    }
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    r.map(|_| ()).map_err(|e| format!("could not open the web browser ({e}); open this address yourself: {url}"))
}

pub fn meta_auth_url(app_id: &str, state: &str) -> String {
    let q: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", app_id)
        .append_pair("redirect_uri", &redirect_uri())
        .append_pair("state", state)
        .append_pair("scope", META_SCOPES)
        .append_pair("response_type", "code")
        .finish();
    format!("https://www.facebook.com/v23.0/dialog/oauth?{q}")
}

pub fn tiktok_auth_url(client_key: &str, state: &str, challenge: &str) -> String {
    let q: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_key", client_key)
        .append_pair("redirect_uri", &redirect_uri())
        .append_pair("state", state)
        .append_pair("scope", TIKTOK_SCOPES)
        .append_pair("response_type", "code")
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .finish();
    format!("https://www.tiktok.com/v2/auth/authorize/?{q}")
}

/// TikTok's desktop flow wants the challenge as hex SHA-256 of the verifier.
pub fn tiktok_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(verifier.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn connect_meta(data: &Path) -> Result<Social, String> {
    let mut cfg = load(data);
    if cfg.meta_app_id.is_empty() || cfg.meta_app_secret.is_empty() {
        return Err("meta app not set".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT)).map_err(|e| format!("port {REDIRECT_PORT}: {e}"))?;
    let state = random_text(24);
    open_url(&meta_auth_url(&cfg.meta_app_id, &state))?;
    let code = wait_for_code(listener, &state)?;
    let a = agent();
    let short = json(a.get(&format!("{GRAPH}/oauth/access_token")).query("client_id", &cfg.meta_app_id).query("client_secret", &cfg.meta_app_secret).query("redirect_uri", redirect_uri()).query("code", &code).call())?;
    let short = s(&short, &["access_token"]);
    // A long-lived sign-in (about 60 days); page keys made from it do not run out.
    let long = json(a.get(&format!("{GRAPH}/oauth/access_token")).query("grant_type", "fb_exchange_token").query("client_id", &cfg.meta_app_id).query("client_secret", &cfg.meta_app_secret).query("fb_exchange_token", &short).call())?;
    let token = s(&long, &["access_token"]);
    let me = json(a.get(&format!("{GRAPH}/me")).query("fields", "name").query("access_token", &token).call())?;
    cfg.meta_name = s(&me, &["name"]);
    cfg.meta_user_token = token.clone();
    cfg.pages = meta_pages(&a, &token)?;
    store(data, &cfg)?;
    Ok(cfg)
}

fn meta_pages(a: &ureq::Agent, token: &str) -> Result<Vec<Page>, String> {
    let v = json(a.get(&format!("{GRAPH}/me/accounts")).query("fields", "id,name,access_token,instagram_business_account{id,username}").query("limit", "100").query("access_token", token).call())?;
    Ok(v["data"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|p| Page {
                    id: s(p, &["id"]),
                    name: s(p, &["name"]),
                    token: s(p, &["access_token"]),
                    ig_id: s(p, &["instagram_business_account", "id"]),
                    ig_username: s(p, &["instagram_business_account", "username"]),
                })
                .collect()
        })
        .unwrap_or_default())
}

fn connect_tiktok(data: &Path) -> Result<Social, String> {
    let mut cfg = load(data);
    if cfg.tiktok_client_key.is_empty() || cfg.tiktok_client_secret.is_empty() {
        return Err("tiktok app not set".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT)).map_err(|e| format!("port {REDIRECT_PORT}: {e}"))?;
    let state = random_text(24);
    let verifier = random_text(64);
    open_url(&tiktok_auth_url(&cfg.tiktok_client_key, &state, &tiktok_challenge(&verifier)))?;
    let code = wait_for_code(listener, &state)?;
    let a = agent();
    let v = json(a.post(&format!("{TIKTOK}/oauth/token/")).send_form([
        ("client_key", cfg.tiktok_client_key.as_str()),
        ("client_secret", cfg.tiktok_client_secret.as_str()),
        ("code", code.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri().as_str()),
        ("code_verifier", verifier.as_str()),
    ]))?;
    take_tiktok_tokens(&mut cfg, &v);
    let me = json(a.get(&format!("{TIKTOK}/user/info/")).query("fields", "display_name").header("Authorization", format!("Bearer {}", cfg.tiktok_access)).call())?;
    cfg.tiktok_name = s(&me, &["data", "user", "display_name"]);
    store(data, &cfg)?;
    Ok(cfg)
}

fn take_tiktok_tokens(cfg: &mut Social, v: &serde_json::Value) {
    cfg.tiktok_access = s(v, &["access_token"]);
    let refresh = s(v, &["refresh_token"]);
    if !refresh.is_empty() {
        cfg.tiktok_refresh = refresh;
    }
    cfg.tiktok_expires = now() + v["expires_in"].as_u64().unwrap_or(86_400);
}

/// TikTok sign-ins last a day; the refresh key renews them for a year.
fn tiktok_token(data: &Path, a: &ureq::Agent) -> Result<String, String> {
    let mut cfg = load(data);
    if cfg.tiktok_access.is_empty() {
        return Err("tiktok not connected".into());
    }
    if now() + 300 < cfg.tiktok_expires {
        return Ok(cfg.tiktok_access);
    }
    let v = json(a.post(&format!("{TIKTOK}/oauth/token/")).send_form([
        ("client_key", cfg.tiktok_client_key.as_str()),
        ("client_secret", cfg.tiktok_client_secret.as_str()),
        ("grant_type", "refresh_token"),
        ("refresh_token", cfg.tiktok_refresh.as_str()),
    ]))?;
    take_tiktok_tokens(&mut cfg, &v);
    store(data, &cfg)?;
    Ok(cfg.tiktok_access)
}

// ---------- publishing ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Target {
    /// "facebook", "instagram" or "tiktok".
    pub platform: String,
    /// "post", "reel" or "story".
    pub kind: String,
    /// The Facebook page (also used to find its Instagram account).
    #[serde(default)]
    pub page_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishJob {
    pub file: PathBuf,
    /// A separate, smaller-shaped file for 9:16 targets, when the edit is not 9:16.
    #[serde(default)]
    pub vertical_file: Option<PathBuf>,
    pub caption: String,
    #[serde(default)]
    pub title: String,
    pub targets: Vec<Target>,
    /// TikTok privacy, from the choices TikTok offers the account.
    #[serde(default = "self_only")]
    pub tiktok_privacy: String,
    #[serde(default)]
    pub tiktok_allow_comments: bool,
}

fn self_only() -> String {
    "SELF_ONLY".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub target: Target,
    pub ok: bool,
    pub message: String,
}

/// Which file a target gets: reels and stories want the tall one.
pub fn file_for<'a>(job: &'a PublishJob, t: &Target) -> &'a Path {
    let tall = t.kind == "reel" || t.kind == "story" || t.platform == "tiktok";
    match (&job.vertical_file, tall) {
        (Some(v), true) => v,
        _ => &job.file,
    }
}

fn size_of(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path).map(|m| m.len()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Sends the whole file to a Meta "rupload" address.
fn rupload(a: &ureq::Agent, url: &str, token: &str, path: &Path) -> Result<serde_json::Value, String> {
    let size = size_of(path)?;
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    json(a.post(url).header("Authorization", format!("OAuth {token}")).header("offset", "0").header("file_size", size.to_string()).header("Content-Type", "application/octet-stream").send(f))
}

fn page<'a>(cfg: &'a Social, id: &str) -> Result<&'a Page, String> {
    cfg.pages.iter().find(|p| p.id == id).ok_or_else(|| "page not found; connect Facebook again".to_string())
}

fn facebook(a: &ureq::Agent, cfg: &Social, t: &Target, path: &Path, job: &PublishJob) -> Result<String, String> {
    let p = page(cfg, &t.page_id)?;
    match t.kind.as_str() {
        "reel" | "story" => {
            let edge = if t.kind == "reel" { "video_reels" } else { "video_stories" };
            let start = json(a.post(&format!("{GRAPH}/{}/{edge}", p.id)).send_form([("upload_phase", "start"), ("access_token", p.token.as_str())]))?;
            let video = s(&start, &["video_id"]);
            let upload = s(&start, &["upload_url"]);
            let upload = if upload.is_empty() { format!("https://rupload.facebook.com/video-upload/v23.0/{video}") } else { upload };
            rupload(a, &upload, &p.token, path)?;
            let mut form = vec![("upload_phase", "finish".to_string()), ("video_id", video.clone()), ("access_token", p.token.clone())];
            if t.kind == "reel" {
                form.push(("video_state", "PUBLISHED".into()));
                form.push(("description", job.caption.clone()));
                if !job.title.is_empty() {
                    form.push(("title", job.title.clone()));
                }
            }
            json(a.post(&format!("{GRAPH}/{}/{edge}", p.id)).send_form(form.iter().map(|(k, v)| (*k, v.as_str()))))?;
            Ok(video)
        }
        _ => {
            // A normal page video: the file goes up with the Resumable
            // Upload API, then the handle is posted to the page.
            let size = size_of(path)?;
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("video.mp4").to_string();
            let session = json(
                a.post(&format!("{GRAPH}/{}/uploads", cfg.meta_app_id))
                    .query("file_name", &name)
                    .query("file_length", size.to_string())
                    .query("file_type", "video/mp4")
                    .query("access_token", &cfg.meta_user_token)
                    .send_empty(),
            )?;
            let id = s(&session, &["id"]);
            let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let up = json(a.post(&format!("{GRAPH}/{id}")).header("Authorization", format!("OAuth {}", cfg.meta_user_token)).header("file_offset", "0").send(f))?;
            let handle = s(&up, &["h"]);
            let mut form = vec![("fbuploader_video_file_chunk", handle), ("description", job.caption.clone()), ("access_token", p.token.clone())];
            if !job.title.is_empty() {
                form.push(("title", job.title.clone()));
            }
            let v = json(a.post(&format!("{GRAPH}/{}/videos", p.id)).send_form(form.iter().map(|(k, v)| (*k, v.as_str()))))?;
            Ok(s(&v, &["id"]))
        }
    }
}

fn instagram(a: &ureq::Agent, cfg: &Social, t: &Target, path: &Path, job: &PublishJob, cancel: &std::sync::atomic::AtomicBool) -> Result<String, String> {
    let p = page(cfg, &t.page_id)?;
    if p.ig_id.is_empty() {
        return Err("no Instagram business or creator account is linked to this page".into());
    }
    let token = &cfg.meta_user_token;
    let mut form: Vec<(&str, String)> = vec![("upload_type", "resumable".into()), ("access_token", token.clone())];
    if t.kind == "story" {
        form.push(("media_type", "STORIES".into()));
    } else {
        form.push(("media_type", "REELS".into()));
        form.push(("caption", job.caption.clone()));
        // A "post" is a reel that also shows in the profile grid.
        form.push(("share_to_feed", if t.kind == "post" { "true" } else { "false" }.into()));
    }
    let c = json(a.post(&format!("{GRAPH}/{}/media", p.ig_id)).send_form(form.iter().map(|(k, v)| (*k, v.as_str()))))?;
    let id = s(&c, &["id"]);
    let uri = s(&c, &["uri"]);
    let uri = if uri.is_empty() { format!("{RUPLOAD_IG}/{id}") } else { uri };
    rupload(a, &uri, token, path)?;
    // Instagram prepares the video before it can go out.
    let until = Instant::now() + Duration::from_secs(15 * 60);
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let st = json(a.get(&format!("{GRAPH}/{id}")).query("fields", "status_code,status").query("access_token", token).call())?;
        match s(&st, &["status_code"]).as_str() {
            "FINISHED" => break,
            "ERROR" | "EXPIRED" => return Err(format!("Instagram could not process the video: {}", s(&st, &["status"]))),
            _ if Instant::now() > until => return Err("Instagram took too long to process the video".into()),
            _ => std::thread::sleep(Duration::from_secs(5)),
        }
    }
    let v = json(a.post(&format!("{GRAPH}/{}/media_publish", p.ig_id)).send_form([("creation_id", id.as_str()), ("access_token", token.as_str())]))?;
    Ok(s(&v, &["id"]))
}

/// TikTok's chunk rules: 5-64 MB pieces, the last one may be bigger;
/// files under 5 MB go up whole. Returns (chunk size, count).
pub fn tiktok_chunks(size: u64) -> (u64, u64) {
    const MB: u64 = 1024 * 1024;
    if size <= 5 * MB {
        return (size, 1);
    }
    let chunk = 10 * MB;
    ((chunk), (size / chunk).max(1))
}

fn tiktok(a: &ureq::Agent, data: &Path, path: &Path, job: &PublishJob, cancel: &std::sync::atomic::AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<String, String> {
    let token = tiktok_token(data, a)?;
    let auth = format!("Bearer {token}");
    let creator = json(a.post(&format!("{TIKTOK}/post/publish/creator_info/query/")).header("Authorization", &auth).header("Content-Type", "application/json; charset=UTF-8").send("{}"))?;
    let options: Vec<String> = creator["data"]["privacy_level_options"].as_array().map(|l| l.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let privacy = if options.contains(&job.tiktok_privacy) { job.tiktok_privacy.clone() } else { "SELF_ONLY".into() };
    let size = size_of(path)?;
    let (chunk, count) = tiktok_chunks(size);
    let body = serde_json::json!({
        "post_info": {
            "title": job.caption.chars().take(2200).collect::<String>(),
            "privacy_level": privacy,
            "disable_comment": !job.tiktok_allow_comments,
            "disable_duet": false,
            "disable_stitch": false,
        },
        "source_info": { "source": "FILE_UPLOAD", "video_size": size, "chunk_size": chunk, "total_chunk_count": count }
    });
    let init = json(a.post(&format!("{TIKTOK}/post/publish/video/init/")).header("Authorization", &auth).header("Content-Type", "application/json; charset=UTF-8").send(body.to_string()))?;
    let publish_id = s(&init, &["data", "publish_id"]);
    let upload = s(&init, &["data", "upload_url"]);
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    for i in 0..count {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let first = i * chunk;
        let last = if i + 1 == count { size - 1 } else { first + chunk - 1 };
        let mut buf = vec![0u8; (last - first + 1) as usize];
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let r = a.put(&upload).header("Content-Type", "video/mp4").header("Content-Range", format!("bytes {first}-{last}/{size}")).send(&buf[..]).map_err(|e| e.to_string())?;
        if r.status().as_u16() >= 400 {
            return Err(format!("TikTok upload failed (HTTP {})", r.status().as_u16()));
        }
        progress((i + 1) as f32 / count as f32);
    }
    let until = Instant::now() + Duration::from_secs(15 * 60);
    loop {
        let st = json(a.post(&format!("{TIKTOK}/post/publish/status/fetch/")).header("Authorization", &auth).header("Content-Type", "application/json; charset=UTF-8").send(serde_json::json!({ "publish_id": publish_id }).to_string()))?;
        match s(&st, &["data", "status"]).as_str() {
            "PUBLISH_COMPLETE" | "SEND_TO_USER_INBOX" => return Ok(publish_id),
            "FAILED" => return Err(format!("TikTok: {}", s(&st, &["data", "fail_reason"]))),
            _ if Instant::now() > until => return Ok(publish_id),
            _ => std::thread::sleep(Duration::from_secs(4)),
        }
    }
}

// ---------- commands ----------

#[derive(Serialize, Clone)]
pub struct PageView {
    id: String,
    name: String,
    ig_username: String,
}

#[derive(Serialize, Clone)]
pub struct SocialView {
    meta_app: bool,
    meta_name: String,
    pages: Vec<PageView>,
    tiktok_app: bool,
    tiktok_name: String,
    redirect: String,
}

fn view(s: &Social) -> SocialView {
    SocialView {
        meta_app: !s.meta_app_id.is_empty() && !s.meta_app_secret.is_empty(),
        meta_name: if s.meta_user_token.is_empty() { String::new() } else { s.meta_name.clone() },
        pages: s.pages.iter().map(|p| PageView { id: p.id.clone(), name: p.name.clone(), ig_username: p.ig_username.clone() }).collect(),
        tiktok_app: !s.tiktok_client_key.is_empty() && !s.tiktok_client_secret.is_empty(),
        tiktok_name: if s.tiktok_access.is_empty() { String::new() } else { s.tiktok_name.clone() },
        redirect: redirect_uri(),
    }
}

#[tauri::command]
pub fn social_status(state: tauri::State<State>) -> SocialView {
    view(&load(&state.data_dir))
}

/// Saves the keys of the user's developer apps. Empty fields keep what is saved.
#[tauri::command]
pub fn social_set_apps(state: tauri::State<State>, platform: String, id: String, secret: String) -> Result<SocialView, String> {
    let mut s = load(&state.data_dir);
    let (id, secret) = (id.trim().to_string(), secret.trim().to_string());
    match platform.as_str() {
        "meta" => {
            if !id.is_empty() {
                s.meta_app_id = id;
            }
            if !secret.is_empty() {
                s.meta_app_secret = secret;
            }
        }
        "tiktok" => {
            if !id.is_empty() {
                s.tiktok_client_key = id;
            }
            if !secret.is_empty() {
                s.tiktok_client_secret = secret;
            }
        }
        _ => return Err("unknown platform".into()),
    }
    store(&state.data_dir, &s)?;
    Ok(view(&s))
}

/// Opens the platform's sign-in page in the browser and waits for the
/// answer. Event: "social-finished" with the new status.
#[tauri::command]
pub fn social_connect(app: AppHandle, state: tauri::State<State>, platform: String) -> Result<(), String> {
    let data = state.data_dir.clone();
    std::thread::spawn(move || {
        let res = match platform.as_str() {
            "meta" => connect_meta(&data),
            "tiktok" => connect_tiktok(&data),
            _ => Err("unknown platform".into()),
        };
        let msg = res.as_ref().err().cloned().unwrap_or_default();
        let _ = app.emit("social-finished", crate::captions::Finished { ok: res.is_ok(), message: msg, result: res.ok().map(|s| view(&s)) });
    });
    Ok(())
}

#[tauri::command]
pub fn social_disconnect(state: tauri::State<State>, platform: String) -> Result<SocialView, String> {
    let mut s = load(&state.data_dir);
    match platform.as_str() {
        "meta" => {
            s.meta_user_token.clear();
            s.meta_name.clear();
            s.pages.clear();
        }
        "tiktok" => {
            s.tiktok_access.clear();
            s.tiktok_refresh.clear();
            s.tiktok_name.clear();
            s.tiktok_expires = 0;
        }
        _ => return Err("unknown platform".into()),
    }
    store(&state.data_dir, &s)?;
    Ok(view(&s))
}

/// TikTok's privacy choices for the signed-in account.
#[tauri::command]
pub async fn tiktok_privacy_options(state: tauri::State<'_, State>) -> Result<Vec<String>, String> {
    let data = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let a = agent();
        let token = tiktok_token(&data, &a)?;
        let v = json(a.post(&format!("{TIKTOK}/post/publish/creator_info/query/")).header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json; charset=UTF-8").send("{}"))?;
        Ok(v["data"]["privacy_level_options"].as_array().map(|l| l.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Uploads the exported video to every chosen place, one after another.
/// Only ever runs when the user presses Publish. Events:
/// "publish-progress" (targets done), then "publish-finished" with one
/// outcome per target.
#[tauri::command]
pub fn publish(app: AppHandle, state: tauri::State<State>, job: PublishJob) -> Result<(), String> {
    if job.targets.is_empty() {
        return Err("nothing chosen".into());
    }
    for t in &job.targets {
        size_of(file_for(&job, t))?;
    }
    let data = state.data_dir.clone();
    let cancel = state.cancel.clone();
    cancel.store(false, std::sync::atomic::Ordering::Relaxed);
    std::thread::spawn(move || {
        let a = agent();
        let cfg = load(&data);
        let total = job.targets.len() as u64 * 100;
        let mut outcomes = vec![];
        for (i, t) in job.targets.iter().enumerate() {
            let base = i as u64 * 100;
            let _ = app.emit("publish-progress", crate::captions::Progress { done: base, total });
            let path = file_for(&job, t);
            let papp = app.clone();
            let res = if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                Err("cancelled".into())
            } else {
                match t.platform.as_str() {
                    "facebook" => facebook(&a, &cfg, t, path, &job),
                    "instagram" => instagram(&a, &cfg, t, path, &job, &cancel),
                    "tiktok" => tiktok(&a, &data, path, &job, &cancel, &mut |p| {
                        let _ = papp.emit("publish-progress", crate::captions::Progress { done: base + (p * 90.0) as u64, total });
                    }),
                    _ => Err("unknown platform".into()),
                }
            };
            outcomes.push(Outcome { target: t.clone(), ok: res.is_ok(), message: res.unwrap_or_else(|e| e) });
        }
        let _ = app.emit("publish-progress", crate::captions::Progress { done: total, total });
        let ok = outcomes.iter().all(|o| o.ok);
        let _ = app.emit("publish-finished", crate::captions::Finished { ok, message: String::new(), result: Some(outcomes) });
    });
    Ok(())
}

/// Opens a platform's own page, for posting by hand.
#[tauri::command]
pub fn open_platform(platform: String) -> Result<(), String> {
    let url = match platform.as_str() {
        "facebook" => "https://www.facebook.com/",
        "instagram" => "https://www.instagram.com/",
        "tiktok" => "https://www.tiktok.com/upload",
        "meta-dev" => "https://developers.facebook.com/apps/",
        "tiktok-dev" => "https://developers.tiktok.com/apps/",
        _ => return Err("unknown platform".into()),
    };
    open_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_follow_tiktok_rules() {
        let mb = 1024 * 1024;
        assert_eq!(tiktok_chunks(3 * mb), (3 * mb, 1));
        assert_eq!(tiktok_chunks(25 * mb), (10 * mb, 2)); // last piece is 15 MB
        assert_eq!(tiktok_chunks(9 * mb), (10 * mb, 1)); // one piece holds all of it
    }

    #[test]
    fn challenge_is_hex_sha256() {
        // SHA-256 of "abc".
        assert_eq!(tiktok_challenge("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn sign_in_urls_carry_our_redirect() {
        let u = meta_auth_url("123", "st");
        assert!(u.starts_with("https://www.facebook.com/"));
        assert!(u.contains("client_id=123") && u.contains("localhost%3A47321%2Fcallback") && u.contains("instagram_content_publish"));
        let t = tiktok_auth_url("key", "st", "ch");
        assert!(t.contains("client_key=key") && t.contains("code_challenge=ch") && t.contains("video.publish"));
    }

    #[test]
    fn tall_targets_get_the_tall_file() {
        let job = PublishJob { file: "wide.mp4".into(), vertical_file: Some("tall.mp4".into()), caption: String::new(), title: String::new(), targets: vec![], tiktok_privacy: self_only(), tiktok_allow_comments: true };
        let t = |p: &str, k: &str| Target { platform: p.into(), kind: k.into(), page_id: String::new() };
        assert_eq!(file_for(&job, &t("facebook", "post")), Path::new("wide.mp4"));
        assert_eq!(file_for(&job, &t("instagram", "reel")), Path::new("tall.mp4"));
        assert_eq!(file_for(&job, &t("tiktok", "post")), Path::new("tall.mp4"));
    }

    #[test]
    fn errors_are_readable() {
        let v = serde_json::json!({"error": {"message": "Invalid token", "code": 190}});
        assert_eq!(platform_error(&v).unwrap(), "Invalid token (190)");
        let ok = serde_json::json!({"error": {"code": "ok", "message": ""}, "data": {}});
        assert!(platform_error(&ok).is_none());
        let t = serde_json::json!({"error": "invalid_grant", "error_description": "Code expired"});
        assert_eq!(platform_error(&t).unwrap(), "invalid_grant: Code expired");
    }

    #[test]
    fn redirect_waits_for_the_right_state() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || wait_for_code(l, "good"));
        std::thread::sleep(Duration::from_millis(100));
        for q in ["state=bad&code=x", "state=good&code=right"] {
            let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            c.write_all(format!("GET /callback?{q} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
            let mut out = String::new();
            let _ = c.read_to_string(&mut out);
        }
        assert_eq!(h.join().unwrap().unwrap(), "right");
    }
}
