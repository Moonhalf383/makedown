import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { LanguageClient, type LanguageClientOptions, type ServerOptions } from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

// 在扩展运行环境中选择随 VSIX 打包的同平台语言服务器。
export function serverPath(extensionPath: string, platform: NodeJS.Platform, arch: string): string {
    const supported = ['win32', 'darwin', 'linux'].includes(platform) && ['x64', 'arm64'].includes(arch);
    if (!supported) {
        throw new Error(`Unsupported platform: ${platform}-${arch}. Install a supported desktop VSIX.`);
    }
    return path.join(extensionPath, 'bin', platform === 'win32' ? 'mkd-lsp.exe' : 'mkd-lsp');
}

// 为打开的 Markfile 和 Markdown 模板启动工作区语言服务器。
export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const executable = serverPath(context.extensionPath, process.platform, process.arch);
    if (!fs.existsSync(executable)) {
        throw new Error(`Bundled mkd-lsp is missing: ${executable}. Reinstall the platform-specific VSIX.`);
    }
    const serverOptions: ServerOptions = { command: executable, options: { cwd: context.extensionPath } };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'markfile' },
            { scheme: 'file', language: 'mkd-template' }
        ]
    };
    client = new LanguageClient('mkd', 'Makedown Language Server', serverOptions, clientOptions);
    await client.start();
}

// 关闭扩展时停止语言服务器子进程。
export async function deactivate(): Promise<void> {
    if (client) {
        await client.stop();
        client = undefined;
    }
}
