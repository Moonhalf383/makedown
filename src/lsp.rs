use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use url::Url;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, GotoDefinitionParams, Position, PublishDiagnosticsParams, Range,
    ReferenceParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Uri,
};

mod workspace;

use crate::formatter::{FormatError, Formatter};
use crate::linter::Linter;
use crate::parser::DiagnosticSeverity as MarkfileSeverity;
use workspace::ProjectView;

// 保存编辑器中的未落盘内容与最新文档版本。
#[derive(Clone)]
struct OpenDocument {
    version: i32,
    text: String,
}

// 管理 LSP 连接中的文档缓存与单文件语言功能。
#[derive(Default)]
struct LanguageServer {
    documents: HashMap<Uri, OpenDocument>,
    views: HashMap<PathBuf, ProjectView>,
    index: Option<ProjectView>,
}

impl LanguageServer {
    // 处理打开、全文更新和关闭文档事件。
    fn notification(&mut self, note: Notification) -> Result<Option<Notification>, Box<dyn Error>> {
        match note.method.as_str() {
            "textDocument/didOpen" => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(note.params)?;
                let document = params.text_document;
                let uri = document.uri;
                self.documents.insert(
                    uri.clone(),
                    OpenDocument {
                        version: document.version,
                        text: document.text,
                    },
                );
                self.invalidate_analysis();
                Ok(Some(self.diagnostics(&uri)))
            }
            "textDocument/didChange" => {
                let params: DidChangeTextDocumentParams = serde_json::from_value(note.params)?;
                let uri = params.text_document.uri;
                if let Some(document) = self.documents.get_mut(&uri)
                    && params.text_document.version > document.version
                    && !params.content_changes.is_empty()
                    && params
                        .content_changes
                        .iter()
                        .all(|change| change.range.is_none())
                {
                    document.text = params
                        .content_changes
                        .last()
                        .expect("change list is nonempty")
                        .text
                        .clone();
                    document.version = params.text_document.version;
                    self.invalidate_analysis();
                    return Ok(Some(self.diagnostics(&uri)));
                }
                Ok(None)
            }
            "textDocument/didClose" => {
                let params: DidCloseTextDocumentParams = serde_json::from_value(note.params)?;
                let uri = params.text_document.uri;
                self.documents.remove(&uri);
                self.invalidate_analysis();
                Ok(Some(Notification::new(
                    "textDocument/publishDiagnostics".into(),
                    PublishDiagnosticsParams {
                        uri,
                        diagnostics: Vec::new(),
                        version: None,
                    },
                )))
            }
            _ => Ok(None),
        }
    }

    // 丢弃已缓存的分析视图和项目索引。
    fn invalidate_analysis(&mut self) {
        self.views.clear();
        self.index = None;
    }

    // 借用或构建单个文件的导入可达分析视图。
    fn view_for(&mut self, path: &Path) -> Option<ProjectView> {
        if !self.views.contains_key(path) {
            let overlays = self.overlays();
            let view = ProjectView::analyze(path, &overlays)?;
            self.views.insert(path.to_path_buf(), view);
        }
        self.views.get(path).cloned()
    }

    // 借用或构建覆盖整个项目及未落盘模块的共享索引。
    fn indexed_view(&mut self, path: &Path) -> Option<ProjectView> {
        let overlays = self.overlays();
        let root = ProjectView::root_for(path, &overlays);
        if self.index.as_ref().is_none_or(|index| index.root() != root) {
            self.index = ProjectView::index(path, &overlays);
        }
        self.index.as_ref().and_then(|index| index.for_path(path))
    }

    // 对其他打开文件重算诊断，使未保存的定义文件即时影响导入者。
    fn refreshed_diagnostics_except(&mut self, changed: &Uri) -> Vec<Notification> {
        let mut uris = self
            .documents
            .keys()
            .filter(|uri| *uri != changed)
            .cloned()
            .collect::<Vec<_>>();
        uris.sort_by_key(|uri| uri.as_str().to_owned());
        uris.into_iter().map(|uri| self.diagnostics(&uri)).collect()
    }

    // 将文件 URI 映射为磁盘路径，不对打开文档执行磁盘读取。
    fn path(uri: &Uri) -> Option<PathBuf> {
        Url::parse(uri.as_str()).ok()?.to_file_path().ok()
    }

    // 收集打开文档的未保存文本作为项目分析优先使用的源码。
    fn overlays(&self) -> HashMap<PathBuf, String> {
        let mut overlays = HashMap::new();
        for (uri, document) in &self.documents {
            if let Some(path) = Self::path(uri) {
                if let Ok(canonical) = path.canonicalize() {
                    overlays.insert(canonical, document.text.clone());
                }
                overlays.insert(path, document.text.clone());
            }
        }
        overlays
    }

    // 为缓存中的完整文本生成语法、质量与跨文件诊断。
    fn diagnostics(&mut self, uri: &Uri) -> Notification {
        let document = self
            .documents
            .get(uri)
            .expect("diagnostics require an open document")
            .clone();
        let path = Self::path(uri).unwrap_or_else(|| PathBuf::from(uri.as_str()));
        let mut diagnostics = Linter::new()
            .lint_source(&path, &document.text)
            .diagnostics()
            .to_vec();
        if let Some(real_path) = Self::path(uri)
            && let Some(view) = self.view_for(&real_path)
        {
            for item in view.diagnostics_for(&real_path) {
                if !diagnostics.iter().any(|existing| existing == &item) {
                    diagnostics.push(item);
                }
            }
        }
        let diagnostics = diagnostics
            .iter()
            .map(|item| {
                let line = item.line().unwrap_or(1).saturating_sub(1);
                let text = document.text.lines().nth(line).unwrap_or("");
                let end = u32::try_from(text.trim_end_matches('\r').encode_utf16().count())
                    .unwrap_or(u32::MAX);
                Diagnostic {
                    range: Range::new(
                        Position::new(u32::try_from(line).unwrap_or(u32::MAX), 0),
                        Position::new(u32::try_from(line).unwrap_or(u32::MAX), end),
                    ),
                    severity: Some(match item.severity() {
                        MarkfileSeverity::Error => DiagnosticSeverity::ERROR,
                        MarkfileSeverity::Warning => DiagnosticSeverity::WARNING,
                    }),
                    code: Some(lsp_types::NumberOrString::String(item.code().to_owned())),
                    source: Some("mkd".into()),
                    message: item.message().to_owned(),
                    ..Default::default()
                }
            })
            .collect();
        Notification::new(
            "textDocument/publishDiagnostics".into(),
            PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics,
                version: Some(document.version),
            },
        )
    }

    // 对打开的文档返回补全、引用、跳转或格式化结果。
    fn request(&mut self, request: Request) -> Response {
        if request.method == "textDocument/definition" {
            let params: GotoDefinitionParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(error) => {
                    return Response::new_err(
                        request.id,
                        lsp_server::ErrorCode::InvalidParams as i32,
                        error.to_string(),
                    );
                }
            };
            let uri = &params.text_document_position_params.text_document.uri;
            let location = Self::path(uri).and_then(|path| {
                self.documents.get(uri)?;
                self.view_for(&path)?
                    .definition(params.text_document_position_params.position)
            });
            return Response::new_ok(request.id, location);
        }
        if request.method == "textDocument/completion" {
            let params: CompletionParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(error) => {
                    return Response::new_err(
                        request.id,
                        lsp_server::ErrorCode::InvalidParams as i32,
                        error.to_string(),
                    );
                }
            };
            let uri = &params.text_document_position.text_document.uri;
            let items = Self::path(uri).and_then(|path| {
                self.documents.get(uri)?;
                Some(
                    self.indexed_view(&path)?
                        .completions(params.text_document_position.position),
                )
            });
            return Response::new_ok(request.id, items);
        }
        if request.method == "textDocument/references" {
            let params: ReferenceParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(error) => {
                    return Response::new_err(
                        request.id,
                        lsp_server::ErrorCode::InvalidParams as i32,
                        error.to_string(),
                    );
                }
            };
            let uri = &params.text_document_position.text_document.uri;
            let locations = Self::path(uri).and_then(|path| {
                self.documents.get(uri)?;
                let view = self.indexed_view(&path)?;
                let target = view.symbol_at(params.text_document_position.position)?;
                Some(view.references(&target, params.context.include_declaration))
            });
            return Response::new_ok(request.id, locations);
        }
        if request.method != "textDocument/formatting" {
            return Response::new_err(
                request.id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                format!("unsupported method `{}`", request.method),
            );
        }
        let params: DocumentFormattingParams = match serde_json::from_value(request.params) {
            Ok(params) => params,
            Err(error) => {
                return Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InvalidParams as i32,
                    error.to_string(),
                );
            }
        };
        let Some(document) = self.documents.get(&params.text_document.uri) else {
            return Response::new_err(
                request.id,
                lsp_server::ErrorCode::InvalidParams as i32,
                "document is not open".into(),
            );
        };
        match Formatter::new().format(&document.text) {
            Ok(formatted) => {
                let edits = if formatted == document.text {
                    Vec::new()
                } else {
                    vec![TextEdit {
                        range: full_range(&document.text),
                        new_text: formatted,
                    }]
                };
                Response::new_ok(request.id, edits)
            }
            Err(FormatError::Parse(_)) => Response::new_err(
                request.id,
                lsp_server::ErrorCode::InvalidParams as i32,
                "cannot format a document with syntax errors".into(),
            ),
            Err(FormatError::ChangedMeaning) => Response::new_err(
                request.id,
                lsp_server::ErrorCode::InternalError as i32,
                "formatting would change parsed meaning".into(),
            ),
        }
    }
}

// 计算包含最后一行末尾的 UTF-16 全文范围。
fn full_range(text: &str) -> Range {
    let normalized = text.replace("\r\n", "\n");
    let line = normalized.rsplit('\n').next().unwrap_or("");
    let row = normalized.bytes().filter(|byte| *byte == b'\n').count();
    Range::new(
        Position::new(0, 0),
        Position::new(
            u32::try_from(row).unwrap_or(u32::MAX),
            u32::try_from(line.encode_utf16().count()).unwrap_or(u32::MAX),
        ),
    )
}

/// 在 stdin/stdout 上提供项目诊断、格式化、跳转、补全与引用查找服务。
pub fn serve() -> Result<(), Box<dyn Error>> {
    let (connection, threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        references_provider: Some(lsp_types::OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![":".into(), ">".into()]),
            ..Default::default()
        }),
        ..Default::default()
    };
    connection.initialize(serde_json::to_value(capabilities)?)?;
    let mut server = LanguageServer::default();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                connection
                    .sender
                    .send(Message::Response(server.request(request)))?;
            }
            Message::Notification(note) => {
                if let Some(notification) = server.notification(note)? {
                    let changed: PublishDiagnosticsParams =
                        serde_json::from_value(notification.params.clone())?;
                    connection
                        .sender
                        .send(Message::Notification(notification))?;
                    for notification in server.refreshed_diagnostics_except(&changed.uri) {
                        connection
                            .sender
                            .send(Message::Notification(notification))?;
                    }
                }
            }
            Message::Response(_) => {}
        }
    }
    drop(connection);
    threads.join()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        TextDocumentContentChangeEvent, TextDocumentItem, VersionedTextDocumentIdentifier,
    };

    // 验证未保存文档更新影响诊断并以新版本发布。
    #[test]
    fn diagnoses_unsaved_full_text_and_clears_on_close() {
        let uri: Uri = "file:///sample.mf".parse().unwrap();
        let mut server = LanguageServer::default();
        let opened = server
            .notification(Notification::new(
                "textDocument/didOpen".into(),
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: uri.clone(),
                        language_id: "mf".into(),
                        version: 1,
                        text: "broken\n".into(),
                    },
                },
            ))
            .unwrap()
            .unwrap();
        let initial: PublishDiagnosticsParams = serde_json::from_value(opened.params).unwrap();
        assert_eq!(
            initial.diagnostics[0].code,
            Some(lsp_types::NumberOrString::String("E002".into()))
        );
        let changed = server
            .notification(Notification::new(
                "textDocument/didChange".into(),
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: 2,
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: "---\n# build\n- spec\n---\n> build\n".into(),
                    }],
                },
            ))
            .unwrap()
            .unwrap();
        let current: PublishDiagnosticsParams = serde_json::from_value(changed.params).unwrap();
        assert_eq!(current.version, Some(2));
        assert!(
            current
                .diagnostics
                .iter()
                .all(|diag| diag.severity != Some(DiagnosticSeverity::ERROR))
        );
        let closed = server
            .notification(Notification::new(
                "textDocument/didClose".into(),
                DidCloseTextDocumentParams {
                    text_document: lsp_types::TextDocumentIdentifier { uri },
                },
            ))
            .unwrap()
            .unwrap();
        let cleared: PublishDiagnosticsParams = serde_json::from_value(closed.params).unwrap();
        assert!(cleared.diagnostics.is_empty());
    }

    // 验证无文件路径的编辑器缓冲区只接收单文件诊断。
    #[test]
    fn untitled_document_does_not_emit_project_file_errors() {
        let uri: Uri = "untitled:Markfile-1".parse().unwrap();
        let mut server = LanguageServer::default();
        let opened = server
            .notification(Notification::new(
                "textDocument/didOpen".into(),
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: "mf".into(),
                        version: 1,
                        text: "---\n# build\n- done\n---\n> build\n".into(),
                    },
                },
            ))
            .unwrap()
            .unwrap();
        let diagnostics: PublishDiagnosticsParams = serde_json::from_value(opened.params).unwrap();
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .all(|item| item.code != Some(lsp_types::NumberOrString::String("P001".into())))
        );
    }

    // 验证诊断按 UTF-16 计算行尾且旧版本更新不会覆盖新内容。
    #[test]
    fn diagnostics_use_utf16_and_ignore_stale_versions() {
        let uri: Uri = "file:///unicode.mf".parse().unwrap();
        let mut server = LanguageServer::default();
        let opened = server
            .notification(Notification::new(
                "textDocument/didOpen".into(),
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: uri.clone(),
                        language_id: "mf".into(),
                        version: 3,
                        text: "😀 broken\n".into(),
                    },
                },
            ))
            .unwrap()
            .unwrap();
        let diagnostics: PublishDiagnosticsParams = serde_json::from_value(opened.params).unwrap();
        assert_eq!(diagnostics.diagnostics[0].range.end.character, 9);
        let stale = server
            .notification(Notification::new(
                "textDocument/didChange".into(),
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: 2,
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: "---\n---\n".into(),
                    }],
                },
            ))
            .unwrap();
        assert!(stale.is_none());
        assert_eq!(server.documents.get(&uri).unwrap().text, "😀 broken\n");
    }

    // 验证未知请求、未打开文档和语法错误均返回协议错误。
    #[test]
    fn formatting_reports_errors_without_terminating_server() {
        let uri: Uri = "file:///absent.mf".parse().unwrap();
        let mut server = LanguageServer::default();
        let request = |method: &str| {
            Request::new(
                1.into(),
                method.into(),
                serde_json::json!({ "textDocument": { "uri": uri },
                "options": { "tabSize": 4, "insertSpaces": true } }),
            )
        };
        let unknown = server.request(request("textDocument/unknown"));
        assert_eq!(unknown.response_result.unwrap_err().code, -32601);
        let unopened = server.request(request("textDocument/formatting"));
        assert_eq!(unopened.response_result.unwrap_err().code, -32602);
        server.documents.insert(
            uri.clone(),
            OpenDocument {
                version: 1,
                text: "broken".into(),
            },
        );
        let invalid = server.request(request("textDocument/formatting"));
        assert_eq!(invalid.response_result.unwrap_err().code, -32602);
    }

    // 验证格式化使用内存文本且 UTF-16 全文范围覆盖非基本平面字符。
    #[test]
    fn formats_cached_document_without_disk_access() {
        let uri: Uri = "file:///absent.mf".parse().unwrap();
        let source = "---\n# build\n描述😀  \n-  spec \n---\n> build";
        let mut server = LanguageServer::default();
        server.documents.insert(
            uri.clone(),
            OpenDocument {
                version: 1,
                text: source.into(),
            },
        );
        let request = Request::new(
            1.into(),
            "textDocument/formatting".into(),
            serde_json::json!({ "textDocument": {"uri": uri}, "options": {"tabSize": 4, "insertSpaces": true} }),
        );
        let response = server.request(request);
        let edits: Vec<TextEdit> =
            serde_json::from_value(response.response_result.unwrap()).unwrap();
        assert_eq!(edits[0].range, full_range(source));
        assert!(edits[0].new_text.contains("描述😀"));
        assert_eq!(full_range("😀").end.character, 2);
    }
}
