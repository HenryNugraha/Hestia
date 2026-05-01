use anyhow::{Context, bail};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const DEFAULT_OUTPUT_NAME: &str = "manifest.json";
const SIGNING_SALT: &[u8] = b"hestia-update-manifest-v1";
const DEFAULT_MANIFEST_URL: &str = "https://github.com/HenryNugraha/Hestia";

#[derive(Debug, Clone)]
pub struct ManifestCliOptions {
    no_prompt: bool,
    app: String,
    file: PathBuf,
    version: String,
    out: PathBuf,
    url: String,
    downloads: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ManifestPayload {
    app: String,
    version: String,
    url: String,
    download: Vec<String>,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManifestDocument {
    app: String,
    version: String,
    url: String,
    download: Vec<String>,
    bytes: u64,
    sha256: String,
    signature: String,
}

pub fn try_run() -> anyhow::Result<bool> {
    let mut args = env::args_os();
    let _exe = args.next();
    let Some(command) = args.next() else {
        return Ok(false);
    };
    let remaining: Vec<OsString> = args.collect();
    let command = command.to_string_lossy();
    let manifest_mode = command == "--manifest";
    let public_key_mode = command == "--public-key";
    let verify_manifest_mode = command == "--verify-manifest";
    if !manifest_mode && !public_key_mode && !verify_manifest_mode {
        return Ok(false);
    }

    let pause_on_error = public_key_mode
        || verify_manifest_mode
        || !remaining.iter().any(|arg| arg == "--no-prompt");
    if let Err(err) = run_manifest_command(public_key_mode, verify_manifest_mode, remaining) {
        eprintln!("Error: {err:#}");
        if pause_on_error {
            let _ = pause_to_close();
        }
        return Err(err);
    }
    Ok(true)
}

fn run_manifest_command(
    public_key_mode: bool,
    verify_manifest_mode: bool,
    remaining: Vec<OsString>,
) -> anyhow::Result<()> {
    if public_key_mode {
        ensure_console(true)?;
        print_manifest_public_key()?;
        pause_to_close()?;
        return Ok(());
    }
    if verify_manifest_mode {
        ensure_console(true)?;
        verify_manifest_file(remaining)?;
        pause_to_close()?;
        return Ok(());
    }

    let mut options = ManifestCliOptions::defaults()?;
    parse_manifest_args(remaining, &mut options)?;
    ensure_console(!options.no_prompt)?;
    if options.no_prompt {
        generate_manifest(&options, true)?;
    } else {
        run_interactive_manifest_menu(&mut options)?;
    }
    Ok(())
}

impl ManifestCliOptions {
    fn defaults() -> anyhow::Result<Self> {
        let current_exe =
            env::current_exe().context("failed to resolve current executable path")?;
        Ok(Self {
            no_prompt: false,
            app: env!("CARGO_PKG_NAME").to_string(),
            file: current_exe,
            version: env!("CARGO_PKG_VERSION").to_string(),
            out: PathBuf::from(format!(".\\{DEFAULT_OUTPUT_NAME}")),
            url: DEFAULT_MANIFEST_URL.to_string(),
            downloads: default_download_links(env!("CARGO_PKG_VERSION")),
        })
    }
}

fn parse_manifest_args(
    args: impl IntoIterator<Item = OsString>,
    options: &mut ManifestCliOptions,
) -> anyhow::Result<()> {
    let mut args = args.into_iter();
    let mut custom_downloads = false;
    while let Some(arg) = args.next() {
        let flag = arg.to_string_lossy();
        match flag.as_ref() {
            "--no-prompt" => options.no_prompt = true,
            "--app" => options.app = next_arg_string(&mut args, "--app")?,
            "--file" => options.file = PathBuf::from(next_arg_string(&mut args, "--file")?),
            "--version" => options.version = next_arg_string(&mut args, "--version")?,
            "--output" => options.out = PathBuf::from(next_arg_string(&mut args, "--output")?),
            "--url" => options.url = next_arg_string(&mut args, "--url")?,
            "--download" => {
                if !custom_downloads {
                    options.downloads.clear();
                    custom_downloads = true;
                }
                options
                    .downloads
                    .push(next_arg_string(&mut args, "--download")?);
            }
            _ => bail!("unknown manifest option: {flag}"),
        }
    }
    Ok(())
}

fn next_arg_string(
    args: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> anyhow::Result<String> {
    args.next()
        .map(|value| value.to_string_lossy().into_owned())
        .with_context(|| format!("missing value for {flag}"))
}

fn run_interactive_manifest_menu(options: &mut ManifestCliOptions) -> anyhow::Result<()> {
    loop {
        print_manifest_menu(options)?;
        match read_choice("Select an entry")?.as_str() {
            "1" => options.app = prompt_text_value("App", &options.app)?,
            "2" => {
                let updated = prompt_text_value("File", &display_path(&options.file))?;
                options.file = PathBuf::from(updated);
            }
            "3" => {
                let old_downloads = default_download_links(&options.version);
                options.version = prompt_text_value("Version", &options.version)?;
                if options.downloads == old_downloads || options.downloads.is_empty() {
                    options.downloads = default_download_links(&options.version);
                }
            }
            "4" => {
                let updated = prompt_text_value("Output", &display_path(&options.out))?;
                options.out = PathBuf::from(updated);
            }
            "5" => options.url = prompt_text_value("URL", &options.url)?,
            "6" => run_downloads_menu(&mut options.downloads)?,
            "0" => match generate_manifest(options, false) {
                Ok(()) => {
                    pause_to_close()?;
                    return Ok(());
                }
                Err(err) => {
                    eprintln!("Error: {err:#}");
                    pause_to_continue()?;
                }
            },
            _ => {
                println!("Invalid selection.");
                println!();
            }
        }
    }
}

fn print_manifest_menu(options: &ManifestCliOptions) -> anyhow::Result<()> {
    clear_screen()?;
    println!("Hestia manifest generator");
    println!("This command creates the update manifest JSON for the selected Hestia binary.");
    println!();
    println!("1. app = {}", display_text(&options.app));
    println!("2. file = {}", display_path(&options.file));
    println!("3. version = {}", display_text(&options.version));
    println!("4. output = {}", display_path(&options.out));
    println!("5. url = {}", display_text(&options.url));
    println!(
        "6. download = {}",
        if options.downloads.is_empty() {
            "[]".to_string()
        } else {
            format!("[{} links]", options.downloads.len())
        }
    );
    println!();
    println!("0. Generate");
    println!();
    Ok(())
}

fn run_downloads_menu(downloads: &mut Vec<String>) -> anyhow::Result<()> {
    loop {
        clear_screen()?;
        println!("Downloads are tried from top to bottom.");
        println!();
        for (index, entry) in downloads.iter().enumerate() {
            println!("{}. {}", index + 1, display_text(entry));
        }
        println!();
        println!("{}. New Link...", downloads.len() + 1);
        println!("0. Back");
        println!();

        let choice = read_choice("Select a download link")?;
        if choice == "0" {
            return Ok(());
        }

        let Ok(index) = choice.parse::<usize>() else {
            println!("Invalid selection.");
            println!();
            continue;
        };
        if index == downloads.len() + 1 {
            let value = prompt_text_value("New download link", "")?;
            if !value.is_empty() {
                downloads.push(value);
            }
            continue;
        }
        if index == 0 || index > downloads.len() {
            println!("Invalid selection.");
            println!();
            continue;
        }

        let current = downloads[index - 1].clone();
        let updated = prompt_text_value("Download link", &current)?;
        if updated.is_empty() {
            downloads.remove(index - 1);
        } else {
            downloads[index - 1] = updated;
        }
    }
}

fn default_download_links(version: &str) -> Vec<String> {
    vec![
        format!("https://hestia.hnawc.com/binary/{version}/hestia.exe"),
        format!("https://github.com/HenryNugraha/Hestia/releases/download/{version}/hestia.exe"),
    ]
}

fn prompt_text_value(label: &str, current: &str) -> anyhow::Result<String> {
    println!("{label} [{}]", display_text(current));
    read_raw_line("Enter value")
}

fn read_choice(label: &str) -> anyhow::Result<String> {
    read_raw_line(label)
}

fn read_raw_line(label: &str) -> anyhow::Result<String> {
    print!("{label}: ");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read console input")?;
    while input.ends_with('\n') || input.ends_with('\r') {
        input.pop();
    }
    Ok(input)
}

fn generate_manifest(options: &ManifestCliOptions, non_interactive: bool) -> anyhow::Result<()> {
    let document = build_manifest_document(options)?;
    let json = serde_json::to_string_pretty(&document).context("failed to serialize manifest")?;
    if let Some(parent) = options.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    fs::write(&options.out, json)
        .with_context(|| format!("failed to write manifest to {}", options.out.display()))?;
    println!("bytes = {}", document.bytes);
    println!("sha256 = {}", document.sha256);
    if non_interactive {
        println!("Generation complete.");
    }
    println!("Manifest saved to {}", options.out.display());
    println!();
    Ok(())
}

fn build_manifest_document(options: &ManifestCliOptions) -> anyhow::Result<ManifestDocument> {
    let payload = build_manifest_payload(options)?;
    let signature = sign_manifest_payload(&payload)?;
    Ok(ManifestDocument {
        app: payload.app,
        version: payload.version,
        url: payload.url,
        download: payload.download,
        bytes: payload.bytes,
        sha256: payload.sha256,
        signature,
    })
}

fn build_manifest_payload(options: &ManifestCliOptions) -> anyhow::Result<ManifestPayload> {
    validate_source_file(&options.file)?;
    let bytes = file_size(&options.file)?;
    let sha256 = sha256_file(&options.file)?;
    Ok(ManifestPayload {
        app: options.app.clone(),
        version: options.version.clone(),
        url: options.url.clone(),
        download: options.downloads.clone(),
        bytes,
        sha256,
    })
}

fn sign_manifest_payload(payload: &ManifestPayload) -> anyhow::Result<String> {
    let passphrase = prompt_signing_passphrase(false)?;
    let signing_key = signing_key_from_passphrase(&passphrase)?;
    ensure_signing_key_matches_embedded_public_key(&signing_key)?;
    let canonical = canonical_manifest_payload(payload)?;
    Ok(BASE64.encode(signing_key.sign(&canonical).to_bytes()))
}

fn print_manifest_public_key() -> anyhow::Result<()> {
    let passphrase = prompt_signing_passphrase(true)?;
    let signing_key = signing_key_from_passphrase(&passphrase)?;
    println!(
        "public_key = {}",
        BASE64.encode(signing_key.verifying_key().to_bytes())
    );
    Ok(())
}

fn verify_manifest_file(args: Vec<OsString>) -> anyhow::Result<()> {
    let path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_NAME));
    let raw =
        fs::read(&path).with_context(|| format!("failed to read manifest {}", path.display()))?;
    let document: ManifestDocument =
        serde_json::from_slice(&raw).context("failed to parse manifest")?;
    verify_manifest_document_signature(&document)?;
    println!("Manifest signature is valid.");
    Ok(())
}

fn ensure_signing_key_matches_embedded_public_key(signing_key: &SigningKey) -> anyhow::Result<()> {
    let public_key = crate::UPDATE_MANIFEST_PUBLIC_KEY_BASE64.trim();
    if public_key.is_empty() {
        return Ok(());
    }

    let expected_public_key = BASE64
        .decode(public_key)
        .context("invalid embedded update manifest public key encoding")?;
    let actual_public_key = signing_key.verifying_key().to_bytes();
    if expected_public_key.as_slice() != actual_public_key {
        bail!("signing passphrase does not match the embedded update public key");
    }
    Ok(())
}

fn verify_manifest_document_signature(document: &ManifestDocument) -> anyhow::Result<()> {
    let public_key = crate::UPDATE_MANIFEST_PUBLIC_KEY_BASE64.trim();
    if public_key.is_empty() {
        bail!("update manifest public key is not configured");
    }

    let public_key_bytes = BASE64
        .decode(public_key)
        .context("invalid update manifest public key encoding")?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("update manifest public key must be 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .context("invalid update manifest public key")?;

    let signature_bytes = BASE64
        .decode(document.signature.trim())
        .context("invalid update manifest signature encoding")?;
    let signature =
        Signature::from_slice(&signature_bytes).context("invalid update manifest signature")?;
    let payload = ManifestPayload {
        app: document.app.clone(),
        version: document.version.clone(),
        url: document.url.clone(),
        download: document.download.clone(),
        bytes: document.bytes,
        sha256: document.sha256.clone(),
    };
    let canonical = canonical_manifest_payload(&payload)?;

    verifying_key
        .verify(&canonical, &signature)
        .context("manifest signature verification failed")
}

fn prompt_signing_passphrase(retry_on_input_error: bool) -> anyhow::Result<String> {
    loop {
        let passphrase = rpassword::prompt_password("Signing passphrase: ")
            .context("failed to read signing passphrase")?;
        let confirmation = rpassword::prompt_password("Re-enter signing passphrase: ")
            .context("failed to read signing passphrase confirmation")?;
        if passphrase != confirmation {
            if retry_on_input_error {
                eprintln!("Signing passphrases do not match. Try again.");
                continue;
            }
            bail!("signing passphrases do not match");
        }
        if passphrase.len() < 16 {
            if retry_on_input_error {
                eprintln!("Signing passphrase must be at least 16 characters. Try again.");
                continue;
            }
            bail!("signing passphrase must be at least 16 characters");
        }
        return Ok(passphrase);
    }
}

fn signing_key_from_passphrase(passphrase: &str) -> anyhow::Result<SigningKey> {
    let argon2 = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(64 * 1024, 3, 1, Some(32))
            .map_err(|err| anyhow::anyhow!("invalid Argon2 params: {err}"))?,
    );
    let mut seed = [0_u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), SIGNING_SALT, &mut seed)
        .map_err(|err| anyhow::anyhow!("failed to derive signing key: {err}"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn canonical_manifest_payload(payload: &ManifestPayload) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec(payload).context("failed to serialize manifest payload")
}

fn validate_source_file(path: &Path) -> anyhow::Result<()> {
    if path.as_os_str().is_empty() {
        bail!("file path cannot be empty");
    }
    if !path.exists() {
        bail!("file does not exist: {}", path.display());
    }
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to read {}", path.display()))?;
    if !metadata.is_file() {
        bail!("path is not a file: {}", path.display());
    }
    Ok(())
}

fn file_size(path: &Path) -> anyhow::Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .len())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn display_text(value: &str) -> String {
    if value.is_empty() {
        "<empty>".to_string()
    } else {
        value.to_string()
    }
}

fn display_path(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        "<empty>".to_string()
    } else {
        path.display().to_string()
    }
}

fn pause_to_close() -> anyhow::Result<()> {
    pause_with_message("Press Enter to close...")
}

fn pause_to_continue() -> anyhow::Result<()> {
    pause_with_message("Press Enter to continue...")
}

fn pause_with_message(message: &str) -> anyhow::Result<()> {
    print!("{message}");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .context("failed to read close confirmation")?;
    Ok(())
}

fn clear_screen() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "cls"])
            .status();
    }
    #[cfg(not(windows))]
    {
        print!("\x1B[2J\x1B[H");
        io::stdout().flush().context("failed to flush stdout")?;
    }
    Ok(())
}

#[cfg(all(windows, not(debug_assertions)))]
fn ensure_console(interactive: bool) -> anyhow::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, FreeConsole, GetConsoleWindow,
    };

    unsafe {
        if GetConsoleWindow() != HWND::default() {
            return Ok(());
        }
        if interactive {
            let _ = FreeConsole();
            if AllocConsole().is_ok() || GetConsoleWindow() != HWND::default() {
                return Ok(());
            }
            bail!("failed to allocate interactive console");
        }
        match AttachConsole(ATTACH_PARENT_PROCESS) {
            Ok(()) => return Ok(()),
            Err(err) if err.code().0 as u32 == 5 => return Ok(()),
            Err(_) => {}
        }
        if GetConsoleWindow() != HWND::default() {
            return Ok(());
        }
        if AllocConsole().is_ok() || GetConsoleWindow() != HWND::default() {
            return Ok(());
        }
        bail!("failed to allocate console");
    }
}

#[cfg(any(not(windows), debug_assertions))]
fn ensure_console(_interactive: bool) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_text_uses_empty_placeholder() {
        assert_eq!(display_text(""), "<empty>");
        assert_eq!(display_text("abc"), "abc");
    }

    #[test]
    fn parse_repeated_download_values() {
        let mut options = ManifestCliOptions::defaults().expect("defaults");
        parse_manifest_args(
            vec![
                OsString::from("--no-prompt"),
                OsString::from("--download"),
                OsString::from("One"),
                OsString::from("--download"),
                OsString::from("Two"),
            ],
            &mut options,
        )
        .expect("parse");
        assert!(options.no_prompt);
        assert_eq!(options.downloads, vec!["One", "Two"]);
    }

    #[test]
    fn default_download_links_use_version() {
        assert_eq!(
            default_download_links("1.0.1"),
            vec![
                "https://hestia.hnawc.com/binary/1.0.1/hestia.exe",
                "https://github.com/HenryNugraha/Hestia/releases/download/1.0.1/hestia.exe",
            ]
        );
    }
}
