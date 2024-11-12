use std::env::consts::EXE_SUFFIX;
use std::env::current_exe;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use crate::app::{App, AppEventProxy, CustomEvent};
use crate::errors::{AppResult, RedefaulterError};
use crate::is_portable;
use crate::popups::start_new_version_popup;

use fs_err as fs;
use http::HeaderMap;
use self_update::cargo_crate_version;
use self_update::get_target;
use self_update::update::ReleaseAsset;
use self_update::version::bump_is_greater;
use sha2::{Digest, Sha512};
use std::sync::mpsc::{self, Receiver};
use tracing::*;

#[derive(Debug)]
enum UpdateCommand {
    CheckForUpdate,
    DownloadUpdate,
    LaunchUpdatedApp,
}
#[derive(Debug)]
pub enum UpdateState {
    Idle,
    UpdateFound(String),
    Downloading,
    // ReadyToLaunch,
}
#[derive(Debug)]
pub enum UpdateReply {
    UpToDate,
    UpdateFound(String),
    // Not used since each time we update the menu, it'd hide it
    // DownloadProgress(f64),
    ReadyToLaunch,
    Error(RedefaulterError),
    // CheckError(RedefaulterError),
}
#[derive(Debug)]
struct UpdateBackend {
    command_rx: Receiver<UpdateCommand>,
    event_proxy: AppEventProxy,
    archive_asset: Option<ReleaseAsset>,
    checksum_asset: Option<ReleaseAsset>,
    current_exe: Option<PathBuf>,
}
impl UpdateBackend {
    fn new(receiver: Receiver<UpdateCommand>, event_proxy: AppEventProxy) -> Self {
        UpdateBackend {
            command_rx: receiver,
            event_proxy,
            archive_asset: None,
            checksum_asset: None,
            current_exe: None,
        }
    }
    fn handle_message(&mut self, msg: UpdateCommand) {
        match msg {
            UpdateCommand::CheckForUpdate => {
                if let Err(e) = self.check_for_update() {
                    error!("Failed checking for update! {e}");
                }
            }
            UpdateCommand::DownloadUpdate => match self.update_executable() {
                Ok(()) => self
                    .event_proxy
                    .send_event(CustomEvent::UpdateReply(UpdateReply::ReadyToLaunch))
                    .expect("Failed to signal update download complete"),

                Err(e) => self
                    .event_proxy
                    .send_event(CustomEvent::UpdateReply(UpdateReply::Error(e)))
                    .expect("Failed to send updater error"),
            },
            UpdateCommand::LaunchUpdatedApp => match self.start_new_version() {
                Ok(()) => {
                    unreachable!()
                }
                Err(e) => self
                    .event_proxy
                    .send_event(CustomEvent::UpdateReply(UpdateReply::Error(e)))
                    .expect("Failed to send updater error"),
            },
        }
    }
    /// Streams the supplied URL's contents into the given File, checking the SHA512 hash of the archive with a supplied checksum by URL
    fn download_and_verify<T: Write + Unpin>(
        &self,
        archive_url: String,
        checksum_url: String,
        mut file: T,
    ) -> AppResult<()> {
        let mut headers = HeaderMap::default();
        headers.insert(
            http::header::ACCEPT,
            "application/octet-stream".parse().unwrap(),
        );
        headers.insert(
            http::header::USER_AGENT,
            "redefaulter/self-update"
                .parse()
                .expect("invalid user-agent"),
        );
        // headers.insert(
        //     http::header::AUTHORIZATION,
        //     (String::from("token ") + "github_pat_xyz")
        //         .parse()
        //         .unwrap(),
        // );

        let client = reqwest::blocking::ClientBuilder::new()
            .default_headers(headers)
            .build()?;

        let resp = client.get(&checksum_url).send()?;
        let size = resp.content_length().unwrap_or(0);
        if !resp.status().is_success() || size == 0 {
            error!("Failed to get archive checksum!");
            return Err(RedefaulterError::HttpStatus(resp.status().as_u16()));
        }

        let content = resp.text()?;
        // Format is `checksum *filename`
        // So we just want the first "word" in the line
        let expected = content
            .split_whitespace()
            .next()
            .ok_or(RedefaulterError::MissingChecksum)?;

        let resp = client.get(&archive_url).send()?;
        let size = resp.content_length().unwrap_or(0);
        if !resp.status().is_success() || size == 0 {
            error!("Failed to get archive!");
            return Err(RedefaulterError::HttpStatus(resp.status().as_u16()));
        }

        // let mut byte_stream = resp.bytes_stream();
        // let mut downloaded: u64 = 0;
        let mut hasher = Sha512::new();
        let mut reader = BufReader::new(resp);

        let mut buffer = [0; 1024 * 8];
        loop {
            match reader.read(&mut buffer) {
                Ok(n) => {
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buffer[..n]);
                    file.write_all(&buffer[..n])?;
                    // downloaded += n as u64;
                    // let percentage = downloaded as f64 / size as f64;
                    // self.event_proxy
                    //     .send_event(CustomEvent::UpdateReply(UpdateReply::DownloadProgress(
                    //         percentage,
                    //     )))
                    //     .unwrap();
                }
                Err(e) => return Err(e.into()),
            }
        }

        // while let Some(item) = byte_stream.next() {
        //     let chunk = item?;
        //     std::io::copy(&mut chunk.as_ref(), &mut file)?;

        // }
        let result = hasher.finalize();
        let checksum = format!("{result:x}");

        if checksum.eq(expected) {
            info!("Update checksum matches! SHA512: {expected}");
            Ok(())
        } else {
            error!(
                "Archive SHA512 checksum mismatch! Expected: {expected} != Calculated: {checksum}"
            );
            Err(RedefaulterError::BadChecksum)
        }
    }
    /// This should never return, unless an error occurs.
    fn start_new_version(&mut self) -> AppResult<()> {
        let current_exe = self.current_exe.take().unwrap();
        // In the happy path, this function won't return
        // since we're ending the process and replacing it with the new one
        Err(restart_process(current_exe))?;

        unreachable!()
    }
    fn update_executable(&mut self) -> AppResult<()> {
        if !is_portable() {
            return Err(RedefaulterError::NotPortable);
        }
        let archive = self.archive_asset.take().expect("Missing archive asset");
        let checksum = self.checksum_asset.take().expect("Missing checksum asset");

        // A lot yoinked from
        // https://github.com/jaemk/self_update/blob/60b3c13533e731650031ee2c410f4bbb4483e845/src/update.rs#L227
        let tmp_archive_dir = tempfile::TempDir::new()?;
        let tmp_archive_path = tmp_archive_dir.path().join(&archive.name);
        let tmp_archive = fs::File::create(&tmp_archive_path)?;
        let mut archive_writer = BufWriter::new(tmp_archive);

        info!("Temp archive location: {}", tmp_archive_path.display());

        self.download_and_verify(
            archive.download_url,
            checksum.download_url,
            &mut archive_writer,
        )?;

        archive_writer.flush()?;

        let bin_name = env!("CARGO_PKG_NAME");
        let bin_name = format!("{}{}", bin_name, EXE_SUFFIX);
        self.current_exe = Some(current_exe()?);

        self_update::Extract::from_source(&tmp_archive_path)
            .extract_file(tmp_archive_dir.path(), &bin_name)?;

        let new_exe = tmp_archive_dir.path().join(bin_name);

        self_replace::self_replace(new_exe)?;

        Ok(())
    }
    /// Returns `true` if a compatible update was found
    fn check_for_update(&mut self) -> AppResult<bool> {
        let bin_name = env!("CARGO_PKG_NAME");
        let current = cargo_crate_version!();
        let release = self_update::backends::github::Update::configure()
            // .auth_token("github_pat_xyz")
            .repo_owner("nullstalgia")
            .repo_name("redefaulter")
            .bin_name(bin_name)
            .current_version(current)
            .build()?
            .get_latest_release()?;
        let newer = bump_is_greater(current, &release.version)?;

        if !newer {
            self.event_proxy
                .send_event(CustomEvent::UpdateReply(UpdateReply::UpToDate))
                .expect("Failed to send up to date message");
            return Ok(false);
        }

        if release.version.contains("pre") {
            error!("Latest was a pre-release? Ignoring...");
            return Ok(false);
        };

        let target = get_target();
        let Some((archive, checksum)) = asset_pair_for(target, &release.assets) else {
            error!("Couldn't find SHA+Archive for given target: {bin_name} {target}");
            return Ok(false);
        };

        self.archive_asset = Some(archive);
        self.checksum_asset = Some(checksum);
        self.event_proxy
            .send_event(CustomEvent::UpdateReply(UpdateReply::UpdateFound(
                release.version.clone(),
            )))
            .expect("Failed to send latest release version");

        Ok(true)
    }
}

/// Returns a pair of ReleaseAssets for the given target from the list of assets
///
/// Returns None if there aren't exactly two files for the given target (either there's too many or too little, we expect one checksum per archive)
///
/// Returns Assets in the order of (Archive, SHA512 Checksum)
fn asset_pair_for(target: &str, releases: &[ReleaseAsset]) -> Option<(ReleaseAsset, ReleaseAsset)> {
    let assets: Vec<&ReleaseAsset> = releases
        .iter()
        .filter(|asset| asset.name.contains(target))
        .collect();

    if assets.len() != 2 {
        return None;
    }

    // I'm gonna assume we get the items in a non-determinate order, so let's sort them ourselves.
    let (checksums, archives): (Vec<&ReleaseAsset>, Vec<&ReleaseAsset>) = assets
        .iter()
        .partition(|asset| asset.name.ends_with(".sha512"));

    // Should be symmetrical since only two total elements
    if checksums.len() != archives.len() {
        return None;
    }

    Some((archives[0].clone(), checksums[0].clone()))
}

fn update_backend_loop(mut actor: UpdateBackend) {
    while let Ok(msg) = actor.command_rx.recv() {
        actor.handle_message(msg);
    }
}

#[derive(Debug)]
pub struct UpdateHandle {
    command_tx: mpsc::Sender<UpdateCommand>,
}

impl UpdateHandle {
    pub fn new(event_proxy: AppEventProxy) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let actor = UpdateBackend::new(command_rx, event_proxy);
        std::thread::spawn(move || update_backend_loop(actor));
        Self { command_tx }
    }
    pub fn query_latest(&self) {
        let msg = UpdateCommand::CheckForUpdate;
        self.command_tx
            .send(msg)
            .expect("Unable to start query for version");
    }
    pub fn download_update(&self) {
        let msg = UpdateCommand::DownloadUpdate;
        self.command_tx
            .send(msg)
            .expect("Unable to start query for version");
    }
    pub fn start_new_version(&self) {
        let msg = UpdateCommand::LaunchUpdatedApp;
        self.command_tx
            .send(msg)
            .expect("Unable to signal for new app launch");
    }
}

// Yoinked from
// https://github.com/lichess-org/fishnet/blob/eac238abbd77b7fc8cacd2d1f7c408252746e2f5/src/main.rs#L399

fn restart_process(current_exe: PathBuf) -> std::io::Error {
    exec(std::process::Command::new(current_exe).args(std::env::args_os().skip(1)))
}

#[cfg(unix)]
fn exec(command: &mut std::process::Command) -> std::io::Error {
    use std::os::unix::process::CommandExt as _;
    // Completely replace the current process image. If successful, execution
    // of the current process stops here.
    command.exec()
}

#[cfg(windows)]
fn exec(command: &mut std::process::Command) -> std::io::Error {
    use std::os::windows::process::CommandExt as _;
    // No equivalent for Unix exec() exists. So create a new independent
    // console instead and terminate the current one:
    // https://docs.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
    let create_new_console = 0x0000_0010;
    match command.creation_flags(create_new_console).spawn() {
        Ok(_) => std::process::exit(libc::EXIT_SUCCESS),
        Err(err) => err,
    }
}

impl App {
    pub fn handle_update_reply(&mut self, reply: UpdateReply) -> AppResult<()> {
        use UpdateReply::*;
        match reply {
            ReadyToLaunch => {
                self.kill_tray_menu();
                start_new_version_popup();
                self.lock_file.take();
                self.updates.start_new_version();
            }
            UpToDate => {
                _ = self.updates.take();
            }
            UpdateFound(version) => {
                if version == self.settings.updates.version_skipped {
                    info!("Update found but version is skipped! (v{version})");
                } else {
                    self.update_state = UpdateState::UpdateFound(version);
                    if let Some(tray) = self.tray_menu.as_ref() {
                        tray.set_icon(self.update_icon.clone())?;
                        self.update_tray_menu()?;
                    }
                }
            }
            Error(e) => {
                error!("Error during update! {e}");
                _ = self.updates.take();
                return Err(e);
            }
        }

        Ok(())
    }
}
