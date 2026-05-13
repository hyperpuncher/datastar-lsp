const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");
const fs = require("fs");
const path = require("path");
const os = require("os");

const BINARY_NAME = process.platform === "win32" ? "datastar-lsp.exe" : "datastar-lsp";
const REPO = "hyperpuncher/datastar-lsp";

let client = undefined;

function getCacheDir() {
	return path.join(os.homedir(), ".datastar-lsp");
}

function getPlatformArch() {
	const arch = process.arch === "arm64" ? "arm64" : "x64";
	let platform;
	switch (process.platform) {
		case "darwin": platform = "darwin"; break;
		case "win32": platform = "windows"; break;
		default: platform = "linux";
	}
	return { platform, arch };
}

async function downloadBinary(version) {
	const cacheDir = getCacheDir();
	const binPath = path.join(cacheDir, BINARY_NAME);

	// Resolve "latest" to actual tag via GitHub API
	let tag = version;
	if (tag === "latest") {
		try {
			const https = require("https");
			tag = await new Promise((resolve, reject) => {
				https.get({
					hostname: "api.github.com",
					path: `/repos/${REPO}/releases/latest`,
					headers: { "User-Agent": "datastar-lsp-vscode" },
				}, (res) => {
					let body = "";
					res.on("data", (d) => body += d);
					res.on("end", () => {
						try {
							resolve(JSON.parse(body).tag_name);
						} catch (_) {
							reject(new Error("failed to parse release"));
						}
					});
				}).on("error", reject);
			});
		} catch (_) {
			// Can't reach GitHub — use existing binary if available
			if (fs.existsSync(binPath)) return binPath;
			vscode.window.showErrorMessage(
				"datastar-lsp: cannot reach GitHub to download binary"
			);
			return null;
		}
	}

	// Check if existing binary matches desired version
	if (fs.existsSync(binPath)) {
		try {
			const { execSync } = require("child_process");
			const output = execSync(`"${binPath}" --version`).toString().trim();
			const installed = output.replace(/^datastar-lsp\s+/, "");
			const wanted = tag.replace(/^v/, "");
			if (installed === wanted) {
				return binPath;
			}
			vscode.window.showInformationMessage(
				`datastar-lsp: updating from ${installed} to ${tag}...`
			);
		} catch (_) {
			// Can't check version (old binary without --version) — re-download
		}
	}

	const { platform, arch } = getPlatformArch();
	const ext = process.platform === "win32" ? ".exe" : "";
	const filename = `datastar-lsp-${platform}-${arch}${ext}`;
	const url = `https://github.com/${REPO}/releases/download/${tag}/${filename}`;

	vscode.window.showInformationMessage(`datastar-lsp: downloading ${filename} (${tag})...`);

	fs.mkdirSync(cacheDir, { recursive: true });

	try {
		const https = require("https");
		const tmpPath = binPath + ".tmp";

		await new Promise((resolve, reject) => {
			function doGet(u, cb) {
				https.get(u, (response) => {
					if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
						doGet(response.headers.location, cb);
						return;
					}
					cb(response);
				}).on("error", reject);
			}
			doGet(url, (response) => {
				if (response.statusCode !== 200) {
					reject(new Error(`HTTP ${response.statusCode}`));
					return;
				}
				const binFile = fs.createWriteStream(tmpPath);
				response.pipe(binFile);
				binFile.on("close", resolve);
				binFile.on("error", reject);
			});
		});

		fs.renameSync(tmpPath, binPath);
		if (process.platform !== "win32") fs.chmodSync(binPath, 0o755);

		vscode.window.showInformationMessage(`datastar-lsp: installed to ${binPath}`);
		return binPath;
	} catch (e) {
		vscode.window.showErrorMessage(`datastar-lsp: download failed: ${e.message}`);
		try { fs.unlinkSync(binPath + ".tmp"); } catch (_) {}
		try { fs.unlinkSync(binPath); } catch (_) {}
		return null;
	}
}

async function resolveBinary() {
	const config = vscode.workspace.getConfiguration("datastar-lsp");
	const custom = config.get("binary", "");
	if (custom && fs.existsSync(custom)) return custom;
	return await downloadBinary(config.get("version", "latest"));
}

async function activate(context) {
	const binary = await resolveBinary();
	if (!binary) return;

	const serverOptions = {
		command: binary,
		transport: TransportKind.stdio,
	};

	const clientOptions = {
		documentSelector: [
			{ scheme: "file", language: "html" },
			{ scheme: "file", language: "javascriptreact" },
			{ scheme: "file", language: "typescriptreact" },
			{ scheme: "file", language: "templ" },
			{ scheme: "file", language: "heex" },
			{ scheme: "file", language: "blade" },
		],
	};

	client = new LanguageClient("datastar-lsp", "Datastar LSP", serverOptions, clientOptions);
	await client.start();
	vscode.window.showInformationMessage("datastar-lsp: ready");
}

async function deactivate() {
	if (client) {
		await client.stop();
	}
}

module.exports = { activate, deactivate };
