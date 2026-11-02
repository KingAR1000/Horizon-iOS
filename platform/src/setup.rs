//! Microsoft device authorization and verified official downloads.
//! Protocol follows Horizon's MIT-licensed launcher/account.rs (Ludovic Marie and contributors).
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
const OAUTH: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const MC: &str = "https://api.minecraftservices.com";
const SCOPE: &str = "XboxLive.signin offline_access";

pub struct Session {
    pub json: Value,
    pub refresh: String,
}
pub struct Installer {
    http: ureq::Agent,
}
impl Default for Installer {
    fn default() -> Self {
        Self {
            http: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(60))
                .build(),
        }
    }
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing service response field: {key}"))
}
fn response(result: std::result::Result<ureq::Response, ureq::Error>) -> Result<(u16, Value)> {
    let r = match result {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r,
        Err(_) => {
            return Err("Connection failed. Check your internet connection and retry.".into());
        }
    };
    let status = r.status();
    let json = serde_json::from_reader(r.into_reader()).map_err(|_| "Invalid service response")?;
    Ok((status, json))
}
fn successful(result: (u16, Value), service: &str) -> Result<Value> {
    if result.0 == 200 {
        Ok(result.1)
    } else {
        Err(format!(
            "{service} refused the request (HTTP {}).",
            result.0
        ))
    }
}
impl Installer {
    pub fn sign_in(
        &self,
        client: &str,
        refresh: Option<&str>,
        cancelled: &AtomicBool,
        status: &(dyn Fn(&str) + Sync),
        device: &dyn Fn(&str, &str),
    ) -> Result<Session> {
        if client.trim().is_empty() {
            return Err("Microsoft sign-in is not configured in this build. The maintainer must supply a Mojang-approved Microsoft client ID.".into());
        }
        let tokens = if let Some(refresh) = refresh {
            successful(
                response(self.http.post(&format!("{OAUTH}/token")).send_form(&[
                    ("client_id", client),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("scope", SCOPE),
                ]))?,
                "Microsoft",
            )?
        } else {
            let code = successful(
                response(
                    self.http
                        .post(&format!("{OAUTH}/devicecode"))
                        .send_form(&[("client_id", client), ("scope", SCOPE)]),
                )?,
                "Microsoft",
            )?;
            device(
                field(&code, "user_code")?,
                field(&code, "verification_uri")?,
            );
            let expires = Instant::now()
                + Duration::from_secs(code["expires_in"].as_u64().unwrap_or(900).min(1800));
            let mut interval = code["interval"].as_u64().unwrap_or(5).max(1);
            loop {
                let until = Instant::now() + Duration::from_secs(interval);
                while Instant::now() < until {
                    if cancelled.load(Ordering::Relaxed) {
                        return Err("Sign-in cancelled.".into());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if Instant::now() >= expires {
                    return Err("Sign-in expired. Try again.".into());
                }
                let (s, v) = response(self.http.post(&format!("{OAUTH}/token")).send_form(&[
                    ("client_id", client),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", field(&code, "device_code")?),
                ]))?;
                match v["error"].as_str() {
                    Some("authorization_pending") => continue,
                    Some("slow_down") => {
                        interval += 5;
                        continue;
                    }
                    _ => {}
                }
                break successful((s, v), "Microsoft")?;
            }
        };
        let refresh = field(&tokens, "refresh_token")?.to_owned();
        status("Signing in to Xbox Live…");
        let xbl=successful(response(self.http.post("https://user.auth.xboxlive.com/user/authenticate").send_json(json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={}",field(&tokens,"access_token")?)},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"})))?,"Xbox Live")?;
        let (s,xsts)=response(self.http.post("https://xsts.auth.xboxlive.com/xsts/authorize").send_json(json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[field(&xbl,"Token")?]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"})))?;
        if let Some(code) = xsts["XErr"].as_u64() {
            return Err(match code {
                2148916233 => "Create an Xbox profile at xbox.com first.",
                2148916238 => "This child account must join a Microsoft family first.",
                _ => "Xbox Live denied sign-in. Check your account at xbox.com.",
            }
            .into());
        }
        let xsts = successful((s, xsts), "Xbox Live")?;
        let uhs = xsts
            .pointer("/DisplayClaims/xui/0/uhs")
            .and_then(Value::as_str)
            .ok_or("Xbox user identity is missing")?;
        let identity = format!("XBL3.0 x={uhs};{}", field(&xsts, "Token")?);
        status("Checking Minecraft ownership…");
        let launcher = response(
            self.http
                .post(&format!("{MC}/launcher/login"))
                .send_json(json!({"xtoken":identity,"platform":"PC_LAUNCHER"})),
        )?;
        let mc = if launcher.0 == 200 {
            launcher.1
        } else {
            successful(
                response(
                    self.http
                        .post(&format!("{MC}/authentication/login_with_xbox"))
                        .send_json(json!({"identityToken":identity})),
                )?,
                "Minecraft (check that the app ID is approved by Mojang)",
            )?
        };
        let token = field(&mc, "access_token")?;
        let get = |path: &str| {
            successful(
                response(
                    self.http
                        .get(&format!("{MC}/{path}"))
                        .set("Authorization", &format!("Bearer {token}"))
                        .call(),
                )?,
                "Minecraft",
            )
        };
        let entitlement = get("entitlements/mcstore")?;
        if !entitlement["items"].as_array().is_some_and(|items| {
            items.iter().any(|i| {
                matches!(
                    i["name"].as_str(),
                    Some("game_minecraft" | "product_minecraft")
                )
            })
        }) {
            return Err("This account does not own Minecraft: Java Edition.".into());
        }
        let profile = get("minecraft/profile")?;
        Ok(Session {
            json: json!({"name":field(&profile,"name")?,"uuid":field(&profile,"id")?,"access_token":token,"user_type":"msa","expires_at":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()+mc["expires_in"].as_u64().unwrap_or(86400)}),
            refresh,
        })
    }
    pub(crate) fn download(
        &self,
        url: &str,
        hash: &str,
        size: u64,
        path: &Path,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        if !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid Mojang download checksum".into());
        }
        let valid = |path: &Path| -> bool {
            let Ok(mut f) = fs::File::open(path) else {
                return false;
            };
            if f.metadata().map(|m| m.len()).ok() != Some(size) {
                return false;
            }
            let mut sha: Box<dyn sha1::digest::DynDigest> = if hash.len() == 40 {
                Box::new(Sha1::new())
            } else {
                Box::new(Sha256::new())
            };
            let mut buf = [0; 65536];
            loop {
                match f.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => sha.update(&buf[..n]),
                    Err(_) => return false,
                }
            }
            sha.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                == hash
        };
        if valid(path) {
            return Ok(());
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("Download cancelled.".into());
        }
        fs::create_dir_all(path.parent().ok_or("Invalid download path")?)
            .map_err(|e| e.to_string())?;
        let temporary = path.with_extension("part");
        let result = (|| {
            let mut source = self
                .http
                .get(url)
                .call()
                .map_err(|_| "Mojang download failed")?
                .into_reader();
            let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
            let mut count = 0u64;
            let mut buf = [0; 65536];
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Download cancelled.".into());
                }
                let n = source.read(&mut buf).map_err(|_| "Download interrupted")?;
                if n == 0 {
                    break;
                }
                count += n as u64;
                if count > size {
                    return Err("Unexpected download size".into());
                }
                file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            }
            file.sync_all().map_err(|e| e.to_string())?;
            if !valid(&temporary) {
                return Err("Mojang download checksum mismatch".into());
            }
            fs::rename(&temporary, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn download_game(
        &self,
        root: &Path,
        cancelled: &AtomicBool,
        status: &(dyn Fn(&str) + Sync),
    ) -> Result<()> {
        let lock: Value = serde_json::from_str(include_str!("../../engine/research-lock.json"))
            .map_err(|_| "Invalid pinned version")?;
        let version = field(&lock["minecraft"], "version")?;
        status("Downloading Mojang version information…");
        let pin = field(&lock["minecraft"], "manifest_sha1")?;
        let url = format!("https://piston-meta.mojang.com/v1/packages/{pin}/{version}.json");
        let response = self
            .http
            .get(&url)
            .call()
            .map_err(|_| "Could not download the pinned version metadata")?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(16 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|_| "Version metadata download failed")?;
        if format!("{:x}", Sha1::digest(&bytes)) != pin {
            return Err("Mojang version metadata checksum mismatch".into());
        }
        let data: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid version metadata")?;
        let version_path = root
            .join("versions")
            .join(version)
            .join(format!("{version}.json"));
        fs::create_dir_all(version_path.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&version_path, bytes).map_err(|e| e.to_string())?;
        for kind in ["client", "server"] {
            status(&format!("Downloading Minecraft {kind}…"));
            let d = &data["downloads"][kind];
            let hash = field(d, "sha1")?;
            if hash != field(&lock["minecraft"], &format!("{kind}_sha1"))? {
                return Err(
                    "Mojang changed the pinned game JAR. Update the engine before downloading."
                        .into(),
                );
            }
            self.download(
                field(d, "url")?,
                hash,
                d["size"].as_u64().ok_or("Missing download size")?,
                &version_path.with_file_name(format!("{kind}.jar")),
                cancelled,
            )?;
        }
        status("Downloading official game libraries…");
        if let Some(libraries) = data["libraries"].as_array() {
            for lib in libraries {
                let a = &lib["downloads"]["artifact"];
                if a.is_null() {
                    continue;
                }
                let relative = Path::new(field(a, "path")?);
                if relative
                    .components()
                    .any(|p| !matches!(p, std::path::Component::Normal(_)))
                {
                    return Err("Unsafe library path in version metadata".into());
                }
                self.download(
                    field(a, "url")?,
                    field(a, "sha1")?,
                    a["size"].as_u64().ok_or("Missing library size")?,
                    &root.join("libraries").join(relative),
                    cancelled,
                )?;
            }
        }
        let index = &data["assetIndex"];
        let index_path = root
            .join("assets/indexes")
            .join(format!("{}.json", field(index, "id")?));
        self.download(
            field(index, "url")?,
            field(index, "sha1")?,
            index["size"].as_u64().ok_or("Missing asset index size")?,
            &index_path,
            cancelled,
        )?;
        let objects: Value =
            serde_json::from_slice(&fs::read(index_path).map_err(|e| e.to_string())?)
                .map_err(|_| "Invalid asset index")?;
        let mut seen = std::collections::HashSet::new();
        let objects: Vec<_> = objects["objects"]
            .as_object()
            .ok_or("Missing asset objects")?
            .values()
            .filter(|object| seen.insert(object["hash"].as_str().unwrap_or_default()))
            .collect();
        let total = objects.len();
        let completed = std::sync::atomic::AtomicUsize::new(0);
        status("Downloading textures, sounds and language assets…");
        std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for group in objects.chunks(objects.len().div_ceil(4).max(1)) {
                let completed = &completed;
                workers.push(scope.spawn(move || -> Result<()> {
                    for obj in group {
                        let hash = field(obj, "hash")?;
                        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                            return Err("Invalid asset hash".into());
                        }
                        self.download(
                            &format!(
                                "https://resources.download.minecraft.net/{}/{}",
                                &hash[..2],
                                hash
                            ),
                            hash,
                            obj["size"].as_u64().ok_or("Missing asset size")?,
                            &root.join("assets/objects").join(&hash[..2]).join(hash),
                            cancelled,
                        )?;
                        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                        if done % 16 == 0 || done == total {
                            status(&format!("Downloading game assets: {done}/{total}"));
                        }
                    }
                    Ok(())
                }));
            }
            for worker in workers {
                worker
                    .join()
                    .map_err(|_| "Asset download worker stopped")??;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    fn fixture(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        (format!("http://{address}/asset"), task)
    }
    #[test]
    fn verifies_download_and_reuses_valid_cache_without_network() {
        let path = std::env::temp_dir()
            .join(format!("horizon-download-{}-valid", std::process::id()))
            .join("object");
        let _ = fs::remove_file(&path);
        let (url, task) = fixture(b"abc");
        let installer = Installer::default();
        let cancelled = AtomicBool::new(false);
        let sha = "a9993e364706816aba3e25717850c26c9cd0d89d";
        installer.download(&url, sha, 3, &path, &cancelled).unwrap();
        task.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abc");
        installer
            .download("http://127.0.0.1:1/unreachable", sha, 3, &path, &cancelled)
            .unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn verifies_sha256_runtime_download() {
        let path = std::env::temp_dir()
            .join(format!("horizon-runtime-download-{}", std::process::id()))
            .join("runtime");
        let (url, task) = fixture(b"abc");
        Installer::default()
            .download(
                &url,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                3,
                &path,
                &AtomicBool::new(false),
            )
            .unwrap();
        task.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abc");
        fs::remove_file(&path).unwrap();
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn rejects_corrupted_asset_without_replacing_existing_file() {
        let path = std::env::temp_dir()
            .join(format!("horizon-download-{}-corrupt", std::process::id()))
            .join("object");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"old").unwrap();
        let (url, task) = fixture(b"bad");
        assert!(
            Installer::default()
                .download(
                    &url,
                    "a9993e364706816aba3e25717850c26c9cd0d89d",
                    3,
                    &path,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        task.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert!(!path.with_extension("part").exists());
        fs::remove_file(&path).unwrap();
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
}
