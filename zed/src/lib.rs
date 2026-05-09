use zed_extension_api::{self as zed, LanguageServerId, Result};

struct DatastarExtension;

impl zed::Extension for DatastarExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // Check PATH first
        if let Some(path) = worktree.which("datastar-lsp") {
            return Ok(zed::Command {
                command: path,
                args: vec![],
                env: vec![],
            });
        }

        // Download from GitHub releases
        let release = zed::latest_github_release(
            "hyperpuncher/datastar-lsp",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, arch) = zed::current_platform();

        let os_name = match os {
            zed::Os::Linux => "linux",
            zed::Os::Mac => "darwin",
            zed::Os::Windows => "windows",
        };
        let arch_name = match arch {
            zed::Architecture::Aarch64 => "arm64",
            zed::Architecture::X8664 => "x64",
            _ => "x64",
        };
        let ext = if matches!(os, zed::Os::Windows) {
            ".exe"
        } else {
            ""
        };

        let asset_name = format!("datastar-lsp-{os_name}-{arch_name}{ext}");

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("no asset matching '{asset_name}' found"))?;

        let version_dir = format!("datastar-lsp-{}", release.version);
        let binary_name = format!("datastar-lsp{ext}");
        let binary_path = format!("{version_dir}/{binary_name}");

        if !std::path::Path::new(&binary_path).exists() {
            let _ = std::fs::create_dir_all(&version_dir);
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(
                &asset.download_url,
                &binary_path,
                zed::DownloadedFileType::Uncompressed,
            )?;
            zed::make_file_executable(&binary_path)?;
        }

        Ok(zed::Command {
            command: binary_path,
            args: vec![],
            env: vec![],
        })
    }
}

zed::register_extension!(DatastarExtension);
