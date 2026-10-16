//! japan-agent: devserver helper for the Japan 2027 trip app.
//!
//! `japan-agent sync`                        pull (rebase) the trip data repo and push local commits
//! `japan-agent push <title> <body> [url]`   send a Web Push to the subscription the PWA stored in the repo
//! `japan-agent pair`                        print a one-time pairing string (host#code) for the PWA
//!
//! Paths come from the environment, with defaults for the devserver:
//! JAPAN_DATA (trip data repo), JAPAN_CONFIG (directory with vapid-private-pkcs8.pem and t3-host),
//! JAPAN_T3_HOME (base dir of the separate T3 instance, default ~/.t3-japan).

use std::{env, fs, path::PathBuf, process::Command, time::Duration};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use web_push_native::{Auth, WebPushBuilder, jwt_simple::algorithms::ES256KeyPair, p256::PublicKey};

const CONTACT: &str = "mailto:15235824+caspervanzoest@users.noreply.github.com";

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()))
}

fn data_dir() -> PathBuf {
    env::var_os("JAPAN_DATA").map(PathBuf::from).unwrap_or_else(|| home().join("git/github/japan-2027"))
}

fn config_dir() -> PathBuf {
    env::var_os("JAPAN_CONFIG").map(PathBuf::from).unwrap_or_else(|| home().join(".config/japan-2027"))
}

/// Runs git in the data repo under an exclusive lock, so the sync timer and a push never fetch at the same
/// time (concurrent fetches can leave FETCH_HEAD with several entries: "Cannot rebase onto multiple branches").
fn git(args: &[&str]) -> Result<String> {
    let dir = data_dir();
    let out = Command::new("flock")
        .arg(dir.join(".git/japan-agent.lock"))
        .arg("git")
        .arg("-C")
        .arg(&dir)
        .args(args)
        .output()
        .context("git starten")?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Pull with rebase; autostash keeps uncommitted edits of a running Claude turn intact.
/// Claude's own git commands do not take our lock, so one retry covers a collision with them.
fn pull() -> Result<()> {
    git(&["pull", "--rebase", "--autostash", "--quiet"]).or_else(|e| {
        eprintln!("pull mislukt, opnieuw: {e}");
        std::thread::sleep(Duration::from_secs(3));
        git(&["pull", "--rebase", "--autostash", "--quiet"]).map(|_| ())
    })?;
    Ok(())
}

/// Pull, then push when there are local commits.
fn sync() -> Result<()> {
    pull()?;
    let ahead: u32 = git(&["rev-list", "--count", "@{u}..HEAD"])?.parse().unwrap_or(0);
    if ahead > 0 {
        git(&["push", "--quiet"])?;
        println!("sync: {ahead} commit(s) gepusht");
    } else {
        println!("sync: up-to-date");
    }
    Ok(())
}

#[derive(Deserialize)]
struct Subscription {
    endpoint: String,
    keys: SubscriptionKeys,
}

#[derive(Deserialize)]
struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}

fn push(title: &str, body: &str, url: Option<&str>) -> Result<()> {
    // Best effort: the PWA may have just written a new subscription.
    if let Err(e) = pull() {
        eprintln!("waarschuwing: pull mislukt, gebruik lokale kopie: {e}");
    }
    let path = data_dir().join("app/push-subscription.json");
    let sub: Subscription = serde_json::from_str(
        &fs::read_to_string(&path).with_context(|| format!("{} lezen (nog niet ingeschreven in de app?)", path.display()))?,
    )?;

    let pem = fs::read_to_string(config_dir().join("vapid-private-pkcs8.pem")).context("VAPID-sleutel lezen")?;
    let key = ES256KeyPair::from_pem(&pem).map_err(|e| anyhow::anyhow!("VAPID-sleutel: {e}"))?;
    let ua_public = PublicKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(sub.keys.p256dh.trim_end_matches('='))?)?;
    let ua_auth = Auth::clone_from_slice(&URL_SAFE_NO_PAD.decode(sub.keys.auth.trim_end_matches('='))?);

    let payload = serde_json::json!({ "title": title, "body": body, "url": url.unwrap_or("./") });
    let request = WebPushBuilder::new(sub.endpoint.parse()?, ua_public, ua_auth)
        .with_valid_duration(Duration::from_secs(12 * 3600))
        .with_vapid(&key, CONTACT)
        .build(payload.to_string())?;

    let (parts, body) = request.into_parts();
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut req = client.post(parts.uri.to_string()).body(body).header("Urgency", "normal");
    for (name, value) in &parts.headers {
        req = req.header(name.as_str(), value.as_bytes());
    }
    let resp = req.send().context("push versturen")?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    match status.as_u16() {
        200..=299 => println!("push: verstuurd ({status})"),
        404 | 410 => bail!("push: inschrijving verlopen ({status}); zet meldingen opnieuw aan in de app"),
        _ => bail!("push: {status} {text}"),
    }
    Ok(())
}

/// One-time T3 pairing credential (10 minutes) plus the public host, pasted into the PWA as `host#code`.
/// The PWA exchanges it for a 30-day token limited to orchestration:read/operate.
fn pair() -> Result<()> {
    let t3_home = env::var_os("JAPAN_T3_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".t3-japan"));
    let mut bins: Vec<PathBuf> = fs::read_dir(t3_home.join("runtime/versions"))
        .context("T3-versies zoeken")?
        .filter_map(|e| e.ok().map(|e| e.path().join("t3")))
        .filter(|p| p.exists())
        .collect();
    bins.sort();
    // Use the version the service actually runs.
    let unit = fs::read_to_string(home().join(".config/systemd/user/t3-japan.service")).unwrap_or_default();
    let bin = bins
        .iter()
        .find(|b| unit.contains(b.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("?")))
        .or(bins.last())
        .context("geen T3-binary gevonden")?;
    let out = Command::new(bin)
        .args(["auth", "pairing", "create", "--ttl", "10m", "--label", "japan-pwa", "--json", "--base-dir"])
        .arg(&t3_home)
        .output()
        .context("t3 starten")?;
    if !out.status.success() {
        bail!("t3 auth pairing create: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).context("t3-uitvoer lezen")?;
    let code = v["credential"].as_str().context("geen credential in t3-uitvoer")?;
    let host = fs::read_to_string(config_dir().join("t3-host")).context("t3-host lezen")?;
    println!("{}#{}", host.trim(), code);
    eprintln!("Geldig tot {} en één keer bruikbaar.", v["expiresAt"].as_str().unwrap_or("?"));
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["sync"] => sync(),
        ["pair"] => pair(),
        ["push", title, body] => push(title, body, None),
        ["push", title, body, url] => push(title, body, Some(url)),
        _ => {
            eprintln!("gebruik: japan-agent sync | japan-agent pair | japan-agent push <titel> <tekst> [url]");
            std::process::exit(2);
        }
    }
}
