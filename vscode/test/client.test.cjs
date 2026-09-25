const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const esbuild = require('esbuild');

// 用最小 VS Code 宿主替身验证客户端传给语言服务器的启动参数。
function loadExtension(records) {
  const source = esbuild.buildSync({
    entryPoints: [path.resolve(__dirname, '..', 'src', 'extension.ts')],
    bundle: true, platform: 'node', format: 'cjs', write: false,
    external: ['vscode', 'vscode-languageclient/node']
  }).outputFiles[0].text;
  const clientModule = { LanguageClient: class {
    constructor(id, name, server, options) { records.push({ id, name, server, options }); }
    async start() { records.push('start'); }
    async stop() { records.push('stop'); }
  } };
  const module = { exports: {} };
  const requireMock = name => {
    if (name === 'vscode') return {};
    if (name === 'vscode-languageclient/node') return clientModule;
    return require(name);
  };
  vm.runInNewContext(source, { module, exports: module.exports, require: requireMock, process, __dirname: path.resolve(__dirname, '..', 'out'), Buffer, setTimeout, clearTimeout });
  return module.exports;
}

test('client selects packaged executable and both document languages', async () => {
  const records = [];
  const extension = loadExtension(records);
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'mkd-vscode-'));
  try {
    const binary = extension.serverPath(temp, process.platform, process.arch);
    fs.mkdirSync(path.dirname(binary), { recursive: true });
    fs.writeFileSync(binary, 'binary');
    assert.equal(binary, path.join(temp, 'bin', process.platform === 'win32' ? 'mkd-lsp.exe' : 'mkd-lsp'));
    await extension.activate({ extensionPath: temp });
    assert.equal(records[0].server.command, binary);
    assert.deepEqual(Array.from(records[0].options.documentSelector, value => value.language), ['markfile', 'mkd-template']);
    assert.equal(records[1], 'start');
    await extension.deactivate();
    assert.equal(records[2], 'stop');
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test('client refuses unsupported platforms and missing bundled binaries', async () => {
  const extension = loadExtension([]);
  assert.throws(() => extension.serverPath('/tmp', 'freebsd', 'x64'), /Unsupported platform/);
  await assert.rejects(() => extension.activate({ extensionPath: '/does-not-exist' }), /Bundled mkd-lsp is missing/);
});
