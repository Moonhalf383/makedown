# 统一管理 Rust 语言服务器和 VS Code 插件的本地构建入口。
.PHONY: build test lsp vscode-deps vscode-build vscode-test vscode-package release-stage clean-vscode

build:
	cargo build --bins

test:
	cargo fmt --check
	cargo test --all-targets
	cargo clippy --all-targets -- -D warnings

lsp:
	cargo build --release --bin mkd-lsp

vscode-deps:
	npm ci --prefix vscode

vscode-build:
	npm run compile --prefix vscode

vscode-test:
	npm test --prefix vscode

# 只允许当前主机的平台包；每个平台需要在对应 GitHub Actions runner 上构建。
vscode-package: lsp vscode-build
	node vscode/scripts/package.cjs $(TARGET)

release-stage:
	cargo build --release --bins
	node vscode/scripts/stage-release.cjs $(TARGET)

clean-vscode:
	node -e "const fs=require('node:fs'); for(const p of ['vscode/bin','vscode/out']) fs.rmSync(p,{recursive:true,force:true});"
