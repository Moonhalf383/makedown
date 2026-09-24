use std::fs;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use std::process::{Command, Stdio};
use url::Url;

use serde_json::{Value, json};

// 为语言服务器编码一条带长度前缀的 JSON-RPC 消息。
fn frame(message: Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

// 解码标准输出中由语言服务器发出的全部 JSON-RPC 消息。
fn responses(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while !bytes.is_empty() {
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let header = std::str::from_utf8(&bytes[..header_end]).unwrap();
        let length: usize = header
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();
        bytes = &bytes[header_end + 4..];
        messages.push(serde_json::from_slice(&bytes[..length]).unwrap());
        bytes = &bytes[length..];
    }
    messages
}

// 向隔离的语言服务器写入通知序列并收集全部响应。
fn exchange(messages: Vec<Value>) -> Vec<Value> {
    let mut server = Command::new(env!("CARGO_BIN_EXE_mkd-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for message in messages {
        server
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&frame(message))
            .unwrap();
    }
    drop(server.stdin.take());
    let output = server.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    responses(&output.stdout)
}

// 验证跨文件诊断随未保存内容变化且跳转使用内存中的目标位置。
#[test]
fn server_refreshes_importers_and_defines_unsaved_targets() {
    let root = std::env::temp_dir().join(format!(
        "mkd-lsp-e2e-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.mf");
    let shared = root.join("shared.mf");
    let importer =
        "> crate::shared::ready as gate\n---\n# build\n> gate\n- complete\n---\n> build\n";
    fs::write(&main, importer).unwrap();
    fs::write(&shared, "---\n# old\n- complete\n---\n> old\n").unwrap();
    let main_uri = Url::from_file_path(&main).unwrap().to_string();
    let shared_uri = Url::from_file_path(&shared).unwrap().to_string();
    let messages = exchange(vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"capabilities":{}}}),
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":main_uri,"languageId":"mf","version":1,"text":importer}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":shared_uri,"languageId":"mf","version":1,"text":"---\n# ready\n- done\n---\n"}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{
            "uri":shared_uri,"version":2},"contentChanges":[{"text":"---\n# ready\n- done\n---\n> ready\n"}]}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
            "textDocument":{"uri":main_uri},"position":{"line":3,"character":3}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":shared_uri}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    ]);
    assert_eq!(
        messages[0]["result"]["capabilities"]["definitionProvider"],
        true
    );
    let importer_updates = messages
        .iter()
        .filter(|msg| {
            msg["method"] == "textDocument/publishDiagnostics" && msg["params"]["uri"] == main_uri
        })
        .collect::<Vec<_>>();
    assert_eq!(importer_updates.len(), 4);
    assert!(
        importer_updates[0]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "P008")
    );
    assert!(
        importer_updates[1]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "P009")
    );
    assert!(
        importer_updates[2]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["code"] != "P008" && item["code"] != "P009")
    );
    assert_eq!(
        messages.iter().find(|msg| msg["id"] == 2).unwrap()["result"]["uri"],
        shared_uri
    );
    assert_eq!(
        messages.iter().find(|msg| msg["id"] == 2).unwrap()["result"]["range"]["start"],
        json!({"line":1,"character":2})
    );
    assert!(
        importer_updates[3]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "P008")
    );
    fs::remove_dir_all(root).unwrap();
}

// 验证补全、跨文件引用及内存更新会重建共享索引。
#[test]
fn server_completes_and_finds_references_after_unsaved_changes() {
    let root = std::env::temp_dir().join(format!(
        "mkd-lsp-index-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.mf");
    let shared = root.join("shared.mf");
    let other = root.join("other.mf");
    let importer = "> crate::shared::ready as gate\n---\n# build\n> gate\n- done\n---\n> build\n";
    let other_text =
        "> crate::shared::ready as another\n---\n# use\n> another\n- done\n---\n> use\n";
    fs::write(&main, importer).unwrap();
    fs::write(&other, other_text).unwrap();
    let main_uri = Url::from_file_path(&main).unwrap().to_string();
    let shared_uri = Url::from_file_path(&shared).unwrap().to_string();
    let other_uri = Url::from_file_path(&other).unwrap().to_string();
    let completion = |id: u32| {
        json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":0,"character":10}}})
    };
    let references = |id: u32, include_declaration: bool| {
        json!({"jsonrpc":"2.0","id":id,"method":"textDocument/references",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":3,"character":3},
            "context":{"includeDeclaration":include_declaration}}})
    };
    let messages = exchange(vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"capabilities":{}}}),
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":main_uri,"languageId":"mf","version":1,"text":importer}}}),
        completion(2),
        references(3, true),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":shared_uri,"languageId":"mf","version":1,
            "text":"---\n# ready\n- done\n---\n> ready\n"}}}),
        completion(4),
        references(5, true),
        references(6, false),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":shared_uri,"version":2},
            "contentChanges":[{"text":"---\n# changed\n- done\n---\n> changed\n"}]}}),
        references(7, true),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":shared_uri,"version":3},
            "contentChanges":[{"text":"---\n# ready\n- done\n---\n> ready\n"}]}}),
        references(8, true),
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":shared_uri}}}),
        references(9, true),
        json!({"jsonrpc":"2.0","id":10,"method":"shutdown","params":null}),
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    ]);
    let reply = |id| messages.iter().find(|item| item["id"] == id).unwrap();
    assert_eq!(
        reply(1)["result"]["capabilities"]["referencesProvider"],
        true
    );
    assert_eq!(
        reply(1)["result"]["capabilities"]["completionProvider"]["triggerCharacters"],
        json!([" ", ":", ">"])
    );
    assert!(
        messages
            .iter()
            .filter(|message| message["method"] == "textDocument/publishDiagnostics")
            .all(|message| message["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .all(|diagnostic| diagnostic["code"] != "P016"))
    );
    assert!(
        !reply(2)["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "crate::shared::ready")
    );
    let items = reply(4)["result"].as_array().unwrap();
    let ready = items
        .iter()
        .find(|item| item["label"] == "crate::shared::ready")
        .unwrap();
    assert_eq!(
        ready["textEdit"]["range"]["start"],
        json!({"line":0,"character":2})
    );
    assert_eq!(
        ready["textEdit"]["range"]["end"],
        json!({"line":0,"character":22})
    );
    assert_eq!(ready["documentation"]["value"], "- done");
    let shared_module = items
        .iter()
        .find(|item| item["label"] == "crate::shared::")
        .unwrap();
    assert_eq!(
        shared_module["documentation"]["value"],
        "public targets:\n- ready"
    );
    assert!(ready["sortText"].as_str().unwrap() < shared_module["sortText"].as_str().unwrap());
    assert_eq!(reply(3)["result"], Value::Null);
    let locations = reply(5)["result"].as_array().unwrap();
    assert_eq!(locations.len(), 6);
    assert_eq!(
        locations
            .iter()
            .filter(|item| item["uri"] == other_uri)
            .count(),
        2
    );
    assert_eq!(
        locations
            .iter()
            .filter(|item| item["uri"] == shared_uri)
            .count(),
        2
    );
    assert_eq!(reply(6)["result"].as_array().unwrap().len(), 5);
    assert_eq!(reply(7)["result"], Value::Null);
    assert_eq!(reply(8)["result"].as_array().unwrap().len(), 6);
    assert_eq!(reply(9)["result"], Value::Null);
    fs::remove_dir_all(root).unwrap();
}

// 验证协议初始化、未保存内容诊断、格式化、关闭及正常退出。
#[test]
fn server_exchanges_lsp_messages_over_stdio() {
    let mut server = Command::new(env!("CARGO_BIN_EXE_mkd-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let uri = "file:///not-on-disk.mf";
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"capabilities":{}}}),
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":uri,"languageId":"mf","version":1,"text":"bad\n"}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{
            "uri":uri,"version":2},"contentChanges":[{"text":"---\n##  run\n- done\n---\n> run"}]}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{
            "textDocument":{"uri":uri},"options":{"tabSize":2,"insertSpaces":true}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    ];
    for message in messages {
        server
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&frame(message))
            .unwrap();
    }
    drop(server.stdin.take());
    let output = server.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages = responses(&output.stdout);
    assert_eq!(messages[0]["result"]["capabilities"]["textDocumentSync"], 1);
    assert_eq!(
        messages[0]["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    let diagnostics = messages
        .iter()
        .filter(|message| message["method"] == "textDocument/publishDiagnostics")
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics[0]["params"]["diagnostics"][0]["code"], "E002");
    assert_eq!(diagnostics[1]["params"]["version"], 2);
    assert!(
        diagnostics[1]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["severity"] != 1)
    );
    assert!(
        diagnostics[2]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let edit = messages.iter().find(|message| message["id"] == 2).unwrap();
    assert!(
        edit["result"][0]["newText"]
            .as_str()
            .unwrap()
            .contains("# run")
    );
    assert!(
        messages
            .iter()
            .any(|message| message["id"] == 3 && message["result"].is_null())
    );
}
