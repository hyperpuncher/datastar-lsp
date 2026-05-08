local M = {}

--- Setup datastar-lsp with auto-download of prebuilt binary.
--- @param opts table|nil Configuration options:
---   cmd - table|nil Override LSP command (default: auto-downloaded binary)
---   filetypes - table|nil Filetypes to attach (default: {"html", "templ", "heex", "blade", "javascriptreact", "typescriptreact"})
---   root_markers - table|nil Root directory markers (default: {".git"})
---   version - string|nil Binary version to download (default: "latest")
---   on_attach - function|nil LSP on_attach callback
function M.setup(opts)
	opts = opts or {}

	local filetypes = opts.filetypes or { "html", "templ", "heex", "blade", "javascriptreact", "typescriptreact" }

	local cmd = opts.cmd or M._get_binary_cmd(opts.version or "latest")
	if not cmd then
		vim.notify("datastar-lsp: failed to find or download binary", vim.log.levels.ERROR)
		return
	end

	local config = {
		cmd = cmd,
		filetypes = filetypes,
		root_markers = opts.root_markers or { ".git" },
	}

	if opts.on_attach then
		config.on_attach = opts.on_attach
	end

	vim.lsp.config("datastar_ls", config)
	vim.lsp.enable("datastar_ls")
end

--- Resolve binary path: check local build, then PATH, then download.
function M._get_binary_cmd(version)
	local name = "datastar-lsp"
	if vim.fn.has("win32") == 1 then
		name = "datastar-lsp.exe"
	end

	-- 1. Check if in PATH
	if vim.fn.executable(name) == 1 then
		return { name }
	end

	-- 3. Download prebuilt binary
	local install_dir = vim.fn.stdpath("data") .. "/datastar-lsp"
	local bin_path = install_dir .. "/" .. name

	if vim.fn.executable(bin_path) ~= 1 then
		local ok = M._download_binary(version, install_dir, bin_path)
		if not ok then
			return nil
		end
	end

	return { bin_path }
end

--- Download prebuilt binary from GitHub releases.
function M._download_binary(version, install_dir, bin_path)
	local arch = M._get_arch()
	local platform = vim.fn.has("mac") == 1 and "darwin" or "linux"
	local ext = vim.fn.has("win32") == 1 and ".exe" or ""
	local filename = "datastar-lsp-" .. platform .. "-" .. arch .. ext

	local url
	if version == "latest" then
		url = "https://github.com/hyperpuncher/datastar-lsp/releases/latest/download/" .. filename
	else
		url = "https://github.com/hyperpuncher/datastar-lsp/releases/download/" .. version .. "/" .. filename
	end

	vim.notify("datastar-lsp: downloading " .. filename .. " ...", vim.log.levels.INFO)

	vim.fn.mkdir(install_dir, "p")

	local ok, _ = pcall(function()
		return vim.fn.system({ "curl", "-fsSL", "-o", bin_path, url })
	end)

	if not ok or vim.v.shell_error ~= 0 then
		vim.notify("datastar-lsp: failed to download binary from " .. url, vim.log.levels.ERROR)
		-- Clean up partial download
		vim.fn.delete(bin_path)
		return false
	end

	vim.fn.setfperm(bin_path, "rwxr-xr-x")
	vim.notify("datastar-lsp: installed to " .. bin_path, vim.log.levels.INFO)
	return true
end

--- Detect architecture string for release filename.
function M._get_arch()
	local uname = vim.fn.system("uname -m"):gsub("%s+", "")
	if uname == "aarch64" or uname == "arm64" then
		return "arm64"
	end
	return "x64"
end

return M
