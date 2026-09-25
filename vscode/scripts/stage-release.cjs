const fs = require('node:fs');
const path = require('node:path');

// 将当前主机编译的两个程序与使用说明整理成 GitHub Release 目录。
const targets = new Set(['linux-x64', 'linux-arm64', 'darwin-x64', 'darwin-arm64', 'win32-x64', 'win32-arm64']);
const target = process.argv[2];
const host = `${process.platform}-${process.arch}`;
if (!targets.has(target) || target !== host) {
  console.error(`This script stages only its host (${host}), not ${target}. Use the matching CI runner.`);
  process.exit(1);
}
const root = path.resolve(__dirname, '..', '..');
const stage = path.join(root, 'dist', `makedown-cli-${target}`);
const suffix = process.platform === 'win32' ? '.exe' : '';
fs.rmSync(stage, { recursive: true, force: true });
fs.mkdirSync(stage, { recursive: true });
for (const name of ['mkd', 'mkd-lsp']) {
  const source = path.join(root, 'target', 'release', `${name}${suffix}`);
  if (!fs.existsSync(source)) {
    console.error(`Build release binaries first: ${source}`);
    process.exit(1);
  }
  fs.copyFileSync(source, path.join(stage, `${name}${suffix}`));
  if (process.platform !== 'win32') fs.chmodSync(path.join(stage, name), 0o755);
}
for (const name of ['README.md', 'LICENSE']) fs.copyFileSync(path.join(root, name), path.join(stage, name));
console.log(stage);
