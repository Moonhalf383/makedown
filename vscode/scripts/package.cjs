const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

// 只打包在当前操作系统和处理器上构建的语言服务器。
const targets = {
  'linux-x64': 'linux-x64',
  'linux-arm64': 'linux-arm64',
  'darwin-x64': 'darwin-x64',
  'darwin-arm64': 'darwin-arm64',
  'win32-x64': 'win32-x64',
  'win32-arm64': 'win32-arm64'
};
const host = `${process.platform}-${process.arch}`;
const requested = process.argv[2] || host;
if (!targets[requested] || requested !== host) {
  console.error(`This script packages only its host (${host}), not ${requested}. Use the matching CI runner.`);
  process.exit(1);
}
const root = path.resolve(__dirname, '..', '..');
const extension = path.resolve(__dirname, '..');
const executable = process.platform === 'win32' ? 'mkd-lsp.exe' : 'mkd-lsp';
const source = path.join(root, 'target', 'release', executable);
if (!fs.existsSync(source)) {
  console.error(`Build the language server first: cargo build --release --bin mkd-lsp (${source})`);
  process.exit(1);
}
const destination = path.join(extension, 'bin');
fs.rmSync(destination, { recursive: true, force: true });
fs.mkdirSync(destination, { recursive: true });
fs.copyFileSync(source, path.join(destination, executable));
if (process.platform !== 'win32') fs.chmodSync(path.join(destination, executable), 0o755);
fs.copyFileSync(path.join(root, 'LICENSE'), path.join(extension, 'LICENSE'));
fs.copyFileSync(path.join(root, 'assets', 'icon.png'), path.join(extension, 'icon.png'));
const vsceManifest = require.resolve('@vscode/vsce/package.json');
const vsce = path.resolve(path.dirname(vsceManifest), require(vsceManifest).bin.vsce);
const result = spawnSync(process.execPath, [vsce, 'package', '--target', requested, '--no-dependencies'], {
  cwd: extension,
  stdio: 'inherit'
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
