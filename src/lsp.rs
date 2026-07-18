//! Minimal standards-based Language Server Protocol adapter for Apex projects.

use crate::{
    editor::{CoverageState, EditorIndex, Location, coverage_overlays, diagnostics},
    project::{Compilation, ProjectCompiler},
    protocol::{read_message, write_message},
    test_runner::{TestOptions, run as run_tests},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

pub fn serve(
    mut reader: impl BufRead,
    mut writer: impl Write,
    initial_root: Option<PathBuf>,
) -> io::Result<()> {
    let mut server = Server::new(initial_root);
    while let Some(message) = read_message(&mut reader)? {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = message.get("id").cloned();
        if method == "exit" {
            break;
        }
        if let Some(id) = id {
            let response = match server.request(method, message.get("params")) {
                Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                Err(error) => json!({
                    "jsonrpc":"2.0",
                    "id":id,
                    "error":{"code":-32603,"message":error}
                }),
            };
            write_message(&mut writer, &response)?;
        } else {
            for notification in server.notification(method, message.get("params")) {
                write_message(&mut writer, &notification)?;
            }
        }
    }
    Ok(())
}

struct Server {
    root: Option<PathBuf>,
    compiler: ProjectCompiler,
    compilation: Option<Compilation>,
    documents: HashMap<String, String>,
}

impl Server {
    fn new(root: Option<PathBuf>) -> Self {
        let mut server = Self {
            root,
            compiler: ProjectCompiler::new(),
            compilation: None,
            documents: HashMap::new(),
        };
        server.refresh();
        server
    }

    fn request(&mut self, method: &str, params: Option<&Value>) -> Result<Value, String> {
        match method {
            "initialize" => {
                if self.root.is_none() {
                    self.root = params
                        .and_then(|params| params.get("rootUri"))
                        .and_then(Value::as_str)
                        .and_then(uri_path)
                        .or_else(|| {
                            params
                                .and_then(|params| params.get("rootPath"))
                                .and_then(Value::as_str)
                                .map(PathBuf::from)
                        });
                    self.refresh();
                }
                Ok(json!({
                    "capabilities":{
                        "textDocumentSync":1,
                        "definitionProvider":true,
                        "referencesProvider":true,
                        "renameProvider":{"prepareProvider":false}
                    },
                    "serverInfo":{"name":"apex-exec","version":env!("CARGO_PKG_VERSION")}
                }))
            }
            "shutdown" => Ok(Value::Null),
            "textDocument/definition" => {
                let (path, line, column) = document_position(params)?;
                Ok(self
                    .index()
                    .and_then(|index| index.definition(path, line, column))
                    .map_or(Value::Null, location_json))
            }
            "textDocument/references" => {
                let (path, line, column) = document_position(params)?;
                let include_declaration = params
                    .and_then(|params| params.pointer("/context/includeDeclaration"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                Ok(self.index().map_or_else(
                    || json!([]),
                    |index| {
                        Value::Array(
                            index
                                .references(path, line, column, include_declaration)
                                .into_iter()
                                .map(location_json)
                                .collect(),
                        )
                    },
                ))
            }
            "textDocument/rename" => {
                let (path, line, column) = document_position(params)?;
                let new_name = params
                    .and_then(|params| params.get("newName"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "rename requires newName".to_owned())?;
                let edits = self
                    .index()
                    .ok_or_else(|| "workspace has no successful project compilation".to_owned())?
                    .rename(path, line, column, new_name)?;
                let mut changes = Map::new();
                for edit in edits {
                    changes
                        .entry(path_uri(&edit.location.path))
                        .or_insert_with(|| Value::Array(Vec::new()))
                        .as_array_mut()
                        .expect("workspace edit buckets are arrays")
                        .push(json!({
                            "range":range_json(&edit.location),
                            "newText":edit.new_text
                        }));
                }
                Ok(json!({"changes":changes}))
            }
            "apex/coverage" => {
                let compilation = self
                    .compilation
                    .as_ref()
                    .ok_or_else(|| "workspace has no successful project compilation".to_owned())?;
                let report = run_tests(compilation, &TestOptions::default())?;
                Ok(Value::Array(
                    coverage_overlays(&report)
                        .into_iter()
                        .map(|overlay| {
                            json!({
                                "uri":path_uri(&compilation.root.join(overlay.path)),
                                "lines":overlay.lines.into_iter().map(|(line,state)| json!({
                                    "line":line - 1,
                                    "covered":state == CoverageState::Covered
                                })).collect::<Vec<_>>()
                            })
                        })
                        .collect(),
                ))
            }
            _ => Err(format!("unsupported LSP request `{method}`")),
        }
    }

    fn notification(&mut self, method: &str, params: Option<&Value>) -> Vec<Value> {
        match method {
            "textDocument/didOpen" => {
                let Some(uri) = params
                    .and_then(|params| params.pointer("/textDocument/uri"))
                    .and_then(Value::as_str)
                else {
                    return Vec::new();
                };
                let text = params
                    .and_then(|params| params.pointer("/textDocument/text"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.documents.insert(uri.to_owned(), text.clone());
                vec![diagnostic_notification(uri, &text)]
            }
            "textDocument/didChange" => {
                let Some(uri) = params
                    .and_then(|params| params.pointer("/textDocument/uri"))
                    .and_then(Value::as_str)
                else {
                    return Vec::new();
                };
                let Some(text) = params
                    .and_then(|params| params.pointer("/contentChanges/0/text"))
                    .and_then(Value::as_str)
                else {
                    return Vec::new();
                };
                self.documents.insert(uri.to_owned(), text.to_owned());
                vec![diagnostic_notification(uri, text)]
            }
            "textDocument/didClose" => {
                let Some(uri) = params
                    .and_then(|params| params.pointer("/textDocument/uri"))
                    .and_then(Value::as_str)
                else {
                    return Vec::new();
                };
                self.documents.remove(uri);
                vec![json!({
                    "jsonrpc":"2.0",
                    "method":"textDocument/publishDiagnostics",
                    "params":{"uri":uri,"diagnostics":[]}
                })]
            }
            "textDocument/didSave" => {
                self.refresh();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn refresh(&mut self) {
        self.compilation = self
            .root
            .as_ref()
            .and_then(|root| self.compiler.compile(root).ok());
    }

    fn index(&self) -> Option<EditorIndex<'_>> {
        self.compilation.as_ref().map(EditorIndex::new)
    }
}

fn diagnostic_notification(uri: &str, source: &str) -> Value {
    let diagnostics = diagnostics(source)
        .into_iter()
        .map(|diagnostic| {
            json!({
                "range":{
                    "start":{"line":diagnostic.line - 1,"character":diagnostic.column - 1},
                    "end":{"line":diagnostic.end_line - 1,"character":diagnostic.end_column - 1}
                },
                "severity":1,
                "source":"apex-exec",
                "message":diagnostic.message
            })
        })
        .collect::<Vec<_>>();
    json!({
        "jsonrpc":"2.0",
        "method":"textDocument/publishDiagnostics",
        "params":{"uri":uri,"diagnostics":diagnostics}
    })
}

fn document_position(params: Option<&Value>) -> Result<(PathBuf, usize, usize), String> {
    let uri = params
        .and_then(|params| params.pointer("/textDocument/uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| "request requires textDocument.uri".to_owned())?;
    let path = uri_path(uri).ok_or_else(|| format!("unsupported document URI `{uri}`"))?;
    let line = params
        .and_then(|params| params.pointer("/position/line"))
        .and_then(Value::as_u64)
        .ok_or_else(|| "request requires position.line".to_owned())? as usize
        + 1;
    let column = params
        .and_then(|params| params.pointer("/position/character"))
        .and_then(Value::as_u64)
        .ok_or_else(|| "request requires position.character".to_owned())? as usize
        + 1;
    Ok((path, line, column))
}

fn location_json(location: Location) -> Value {
    json!({"uri":path_uri(&location.path),"range":range_json(&location)})
}

fn range_json(location: &Location) -> Value {
    json!({
        "start":{"line":location.line - 1,"character":location.column - 1},
        "end":{"line":location.end_line - 1,"character":location.end_column - 1}
    })
}

fn path_uri(path: &Path) -> String {
    format!("file://{}", path.display()).replace(' ', "%20")
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(path)))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn protocol_advertises_editor_features_and_publishes_inline_diagnostics() {
        let mut input = Vec::new();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({
                "jsonrpc":"2.0",
                "method":"textDocument/didOpen",
                "params":{"textDocument":{
                    "uri":"file:///tmp/Broken.apex",
                    "text":"Integer value = missing;"
                }}
            }),
            json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ] {
            write_message(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        serve(BufReader::new(Cursor::new(input)), &mut output, None).unwrap();
        let mut reader = BufReader::new(Cursor::new(output));
        let initialize = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(
            initialize.pointer("/result/capabilities/definitionProvider"),
            Some(&Value::Bool(true))
        );
        let published = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(
            published.get("method").and_then(Value::as_str),
            Some("textDocument/publishDiagnostics")
        );
        assert!(
            published
                .pointer("/params/diagnostics/0/message")
                .and_then(Value::as_str)
                .unwrap()
                .contains("unknown variable")
        );
        assert!(read_message(&mut reader).unwrap().is_some());
    }

    #[test]
    fn protocol_serves_navigation_rename_references_coverage_and_document_lifecycle() {
        let root = std::fs::canonicalize("examples/milestone6-project").unwrap();
        let test_file = root.join("force-app/main/default/classes/CalculatorTest.cls");
        let test_uri = path_uri(&test_file);
        let scratch_uri = "file:///tmp/EditorScratch.apex";
        let mut input = Vec::new();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "rootUri":path_uri(&root)
            }}),
            json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
                "textDocument":{"uri":test_uri},"position":{"line":12,"character":42}
            }}),
            json!({"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{
                "textDocument":{"uri":test_uri},"position":{"line":12,"character":42},
                "context":{"includeDeclaration":true}
            }}),
            json!({"jsonrpc":"2.0","id":4,"method":"textDocument/rename","params":{
                "textDocument":{"uri":test_uri},"position":{"line":12,"character":42},
                "newName":"sum"
            }}),
            json!({"jsonrpc":"2.0","id":5,"method":"apex/coverage","params":{}}),
            json!({"jsonrpc":"2.0","id":6,"method":"unknown/request","params":{}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":scratch_uri,"text":"Integer value = missing;"}
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":scratch_uri},
                "contentChanges":[{"text":"Integer value = 1;"}]
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":scratch_uri}
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
                "textDocument":{"uri":test_uri}
            }}),
            json!({"jsonrpc":"2.0","id":7,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ] {
            write_message(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        serve(BufReader::new(Cursor::new(input)), &mut output, None).unwrap();
        let mut reader = BufReader::new(Cursor::new(output));
        let mut messages = Vec::new();
        while let Some(message) = read_message(&mut reader).unwrap() {
            messages.push(message);
        }

        let response = |id| {
            messages
                .iter()
                .find(|message| message.get("id").and_then(Value::as_u64) == Some(id))
                .unwrap()
        };
        assert!(
            response(2)
                .pointer("/result/uri")
                .and_then(Value::as_str)
                .unwrap()
                .ends_with("Calculator.cls")
        );
        assert_eq!(
            response(3)
                .pointer("/result")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            response(4)
                .pointer("/result/changes")
                .and_then(Value::as_object)
                .unwrap()
                .values()
                .flat_map(|edits| edits.as_array().unwrap())
                .count(),
            3
        );
        assert!(
            response(5)
                .pointer("/result/0/lines")
                .and_then(Value::as_array)
                .is_some_and(|lines| !lines.is_empty())
        );
        assert_eq!(
            response(6).pointer("/error/code").and_then(Value::as_i64),
            Some(-32603)
        );
        let published = messages
            .iter()
            .filter(|message| {
                message.get("method").and_then(Value::as_str)
                    == Some("textDocument/publishDiagnostics")
            })
            .collect::<Vec<_>>();
        assert_eq!(published.len(), 3);
        assert_eq!(
            published[0]
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            1
        );
        assert!(
            published[1]
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .unwrap()
                .is_empty()
        );
    }
}
