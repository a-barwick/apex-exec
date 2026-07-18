//! Debug Adapter Protocol server backed by deterministic runtime snapshots.

use crate::{
    debugger::{DebuggerSession, Stop, StopReason},
    protocol::{read_message, write_message},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
};

pub fn serve(mut reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    let mut server = Server::default();
    while let Some(request) = read_message(&mut reader)? {
        let command = request
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let request_seq = request.get("seq").and_then(Value::as_u64).unwrap_or(0);
        for message in server.handle(request_seq, command, request.get("arguments")) {
            write_message(&mut writer, &message)?;
        }
        if command == "disconnect" {
            break;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Server {
    sequence: u64,
    session: Option<DebuggerSession>,
    stop_on_entry: bool,
}

impl Server {
    fn handle(&mut self, request_seq: u64, command: &str, arguments: Option<&Value>) -> Vec<Value> {
        match self.request(command, arguments) {
            Ok((body, events)) => {
                let response = self.response(request_seq, command, true, body, None);
                let mut messages = vec![response];
                messages.extend(events);
                messages
            }
            Err(error) => {
                let response = self.response(request_seq, command, false, Value::Null, Some(error));
                vec![response]
            }
        }
    }

    fn request(
        &mut self,
        command: &str,
        arguments: Option<&Value>,
    ) -> Result<(Value, Vec<Value>), String> {
        match command {
            "initialize" => {
                let initialized = self.event("initialized", None);
                Ok((
                    json!({
                        "supportsConfigurationDoneRequest":true,
                        "supportsStepBack":false,
                        "supportsTerminateRequest":true
                    }),
                    vec![initialized],
                ))
            }
            "launch" => {
                let program = arguments
                    .and_then(|arguments| arguments.get("program"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "launch requires arguments.program".to_owned())?;
                self.stop_on_entry = arguments
                    .and_then(|arguments| arguments.get("stopOnEntry"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let path = PathBuf::from(program);
                self.session = Some(if path.is_dir() {
                    let target = arguments
                        .and_then(|arguments| arguments.get("target"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            "project launch requires a Class.method target".to_owned()
                        })?;
                    let compilation =
                        crate::project::compile(&path).map_err(|error| error.render())?;
                    DebuggerSession::for_project(&compilation, target)?
                } else {
                    let source = fs::read_to_string(&path)
                        .map_err(|error| format!("failed to read `{program}`: {error}"))?;
                    DebuggerSession::for_script(path, &source)
                        .map_err(|diagnostic| diagnostic.render(program, &source))?
                });
                Ok((Value::Null, Vec::new()))
            }
            "setBreakpoints" => {
                let source_path = arguments
                    .and_then(|arguments| arguments.pointer("/source/path"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "setBreakpoints requires source.path".to_owned())?;
                let lines = arguments
                    .and_then(|arguments| arguments.get("breakpoints"))
                    .and_then(Value::as_array)
                    .map(|breakpoints| {
                        breakpoints
                            .iter()
                            .filter_map(|breakpoint| breakpoint.get("line").and_then(Value::as_u64))
                            .map(|line| line as usize)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let breakpoints = self
                    .session_mut()?
                    .set_breakpoints(source_path, &lines)
                    .into_iter()
                    .map(|breakpoint| {
                        json!({
                            "verified":breakpoint.verified,
                            "line":breakpoint.actual_line.unwrap_or(breakpoint.line)
                        })
                    })
                    .collect::<Vec<_>>();
                Ok((json!({"breakpoints":breakpoints}), Vec::new()))
            }
            "configurationDone" => {
                let stop_on_entry = self.stop_on_entry;
                let stop = self.session_mut()?.start(stop_on_entry);
                Ok((Value::Null, self.stop_events(stop)))
            }
            "threads" => Ok((
                json!({"threads":[{"id":1,"name":"Apex deterministic runtime"}]}),
                Vec::new(),
            )),
            "stackTrace" => {
                let frames = self
                    .session_ref()?
                    .stack_frames()
                    .into_iter()
                    .map(|frame| {
                        json!({
                            "id":frame.id,
                            "name":frame.name,
                            "source":{
                                "name":frame.position.path.file_name().and_then(|name| name.to_str()).unwrap_or("<source>"),
                                "path":frame.position.path
                            },
                            "line":frame.position.line,
                            "column":frame.position.column
                        })
                    })
                    .collect::<Vec<_>>();
                let total_frames = frames.len();
                Ok((
                    json!({"stackFrames":frames,"totalFrames":total_frames}),
                    Vec::new(),
                ))
            }
            "scopes" => Ok((
                json!({
                    "scopes":[{
                        "name":"Locals",
                        "presentationHint":"locals",
                        "variablesReference":1,
                        "expensive":false
                    }]
                }),
                Vec::new(),
            )),
            "variables" => {
                let variables = self
                    .session_ref()?
                    .variables()
                    .iter()
                    .map(|variable| {
                        json!({
                            "name":variable.name,
                            "type":variable.type_name,
                            "value":variable.value,
                            "variablesReference":0
                        })
                    })
                    .collect::<Vec<_>>();
                Ok((json!({"variables":variables}), Vec::new()))
            }
            "continue" => {
                let stop = self.session_mut()?.continue_execution();
                Ok((json!({"allThreadsContinued":true}), self.stop_events(stop)))
            }
            "next" => {
                let stop = self.session_mut()?.step_over();
                Ok((Value::Null, self.stop_events(stop)))
            }
            "stepIn" => {
                let stop = self.session_mut()?.step_in();
                Ok((Value::Null, self.stop_events(stop)))
            }
            "stepOut" => {
                let stop = self.session_mut()?.step_out();
                Ok((Value::Null, self.stop_events(stop)))
            }
            "apex/database" => {
                let inspection = self.session_ref()?.inspect_database();
                Ok((
                    json!({
                        "visibleTransactionEvents":inspection.visible_transaction_events,
                        "dml":inspection.dml_events.into_iter().map(|event| json!({
                            "operation":format!("{:?}",event.operation),
                            "objects":event.objects,
                            "records":event.records,
                            "succeeded":event.succeeded
                        })).collect::<Vec<_>>()
                    }),
                    Vec::new(),
                ))
            }
            "apex/transactionTimeline" => Ok((
                json!({
                    "events":self.session_ref()?.visible_timeline().iter().map(|event| {
                        format!("{event:?}")
                    }).collect::<Vec<_>>()
                }),
                Vec::new(),
            )),
            "disconnect" | "terminate" => {
                let terminated = self.event("terminated", None);
                Ok((Value::Null, vec![terminated]))
            }
            _ => Err(format!("unsupported DAP request `{command}`")),
        }
    }

    fn stop_events(&mut self, stop: Stop) -> Vec<Value> {
        match stop.reason {
            StopReason::Complete => {
                let output = self
                    .session
                    .as_ref()
                    .map(|session| session.output().to_vec())
                    .unwrap_or_default();
                let mut events = output
                    .into_iter()
                    .map(|line| {
                        self.event(
                            "output",
                            Some(json!({"category":"stdout","output":format!("{line}\n")})),
                        )
                    })
                    .collect::<Vec<_>>();
                events.push(self.event("terminated", None));
                events
            }
            StopReason::Exception => {
                let stopped = self.event(
                    "stopped",
                    Some(json!({
                        "reason":"exception",
                        "threadId":1,
                        "allThreadsStopped":true,
                        "description":stop.description
                    })),
                );
                vec![stopped]
            }
            reason => {
                let stopped = self.event(
                    "stopped",
                    Some(json!({
                        "reason":match reason {
                            StopReason::Entry => "entry",
                            StopReason::Breakpoint => "breakpoint",
                            StopReason::Step => "step",
                            StopReason::Exception | StopReason::Complete => unreachable!()
                        },
                        "threadId":1,
                        "allThreadsStopped":true
                    })),
                );
                vec![stopped]
            }
        }
    }

    fn response(
        &mut self,
        request_seq: u64,
        command: &str,
        success: bool,
        body: Value,
        message: Option<String>,
    ) -> Value {
        let sequence = self.next_sequence();
        json!({
            "seq":sequence,
            "type":"response",
            "request_seq":request_seq,
            "success":success,
            "command":command,
            "body":body,
            "message":message
        })
    }

    fn event(&mut self, event: &str, body: Option<Value>) -> Value {
        let sequence = self.next_sequence();
        json!({"seq":sequence,"type":"event","event":event,"body":body})
    }

    fn next_sequence(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }

    fn session_ref(&self) -> Result<&DebuggerSession, String> {
        self.session
            .as_ref()
            .ok_or_else(|| "debug session has not been launched".to_owned())
    }

    fn session_mut(&mut self) -> Result<&mut DebuggerSession, String> {
        self.session
            .as_mut()
            .ok_or_else(|| "debug session has not been launched".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{read_message, write_message};
    use std::{
        io::{BufReader, Cursor},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn dap_launch_breakpoint_variables_step_and_terminate_follow_protocol_order() {
        let path = std::env::temp_dir().join(format!(
            "apex-exec-dap-{}.apex",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "Integer total = 1;\ntotal++;\nSystem.debug(total);").unwrap();
        let mut input = Vec::new();
        for message in [
            json!({"seq":1,"type":"request","command":"initialize","arguments":{}}),
            json!({"seq":2,"type":"request","command":"launch","arguments":{
                "program":path,"stopOnEntry":true
            }}),
            json!({"seq":3,"type":"request","command":"setBreakpoints","arguments":{
                "source":{"path":path},"breakpoints":[{"line":2}]
            }}),
            json!({"seq":4,"type":"request","command":"configurationDone"}),
            json!({"seq":5,"type":"request","command":"continue"}),
            json!({"seq":6,"type":"request","command":"threads"}),
            json!({"seq":7,"type":"request","command":"stackTrace","arguments":{"threadId":1}}),
            json!({"seq":8,"type":"request","command":"scopes","arguments":{"frameId":1}}),
            json!({"seq":9,"type":"request","command":"variables","arguments":{"variablesReference":1}}),
            json!({"seq":10,"type":"request","command":"next","arguments":{"threadId":1}}),
            json!({"seq":11,"type":"request","command":"apex/database"}),
            json!({"seq":12,"type":"request","command":"apex/transactionTimeline"}),
            json!({"seq":13,"type":"request","command":"continue"}),
            json!({"seq":14,"type":"request","command":"disconnect"}),
        ] {
            write_message(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        serve(BufReader::new(Cursor::new(input)), &mut output).unwrap();
        let mut reader = BufReader::new(Cursor::new(output));
        let mut messages = Vec::new();
        while let Some(message) = read_message(&mut reader).unwrap() {
            messages.push(message);
        }
        assert!(messages.iter().any(|message| {
            message.get("event").and_then(Value::as_str) == Some("initialized")
        }));
        assert!(messages.iter().any(|message| {
            message.pointer("/body/reason").and_then(Value::as_str) == Some("breakpoint")
        }));
        assert!(messages.iter().any(|message| {
            message
                .pointer("/body/variables/0/value")
                .and_then(Value::as_str)
                == Some("1")
        }));
        assert!(
            messages.iter().any(|message| {
                message.get("event").and_then(Value::as_str) == Some("terminated")
            })
        );
        assert!(messages.iter().any(|message| {
            message
                .pointer("/body/stackFrames/0/name")
                .and_then(Value::as_str)
                == Some("<anonymous>")
        }));
        assert!(messages.iter().any(|message| {
            message
                .pointer("/body/scopes/0/variablesReference")
                .and_then(Value::as_u64)
                == Some(1)
        }));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn dap_project_launch_serializes_database_and_transaction_timeline_requests() {
        let root = std::fs::canonicalize("examples/milestone9-project").unwrap();
        let source = root.join("force-app/main/default/classes/TriggerDemo.cls");
        let mut input = Vec::new();
        for message in [
            json!({"seq":1,"type":"request","command":"initialize","arguments":{}}),
            json!({"seq":2,"type":"request","command":"launch","arguments":{
                "program":root,"target":"TriggerDemo.run","stopOnEntry":false
            }}),
            json!({"seq":3,"type":"request","command":"setBreakpoints","arguments":{
                "source":{"path":source},"breakpoints":[{"line":6}]
            }}),
            json!({"seq":4,"type":"request","command":"configurationDone"}),
            json!({"seq":5,"type":"request","command":"apex/database"}),
            json!({"seq":6,"type":"request","command":"continue"}),
            json!({"seq":7,"type":"request","command":"apex/database"}),
            json!({"seq":8,"type":"request","command":"apex/transactionTimeline"}),
            json!({"seq":9,"type":"request","command":"terminate"}),
            json!({"seq":10,"type":"request","command":"disconnect"}),
        ] {
            write_message(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        serve(BufReader::new(Cursor::new(input)), &mut output).unwrap();
        let mut reader = BufReader::new(Cursor::new(output));
        let mut messages = Vec::new();
        while let Some(message) = read_message(&mut reader).unwrap() {
            messages.push(message);
        }
        let response = |request_seq| {
            messages
                .iter()
                .find(|message| {
                    message.get("request_seq").and_then(Value::as_u64) == Some(request_seq)
                })
                .unwrap()
        };
        assert_eq!(
            response(5)
                .pointer("/body/visibleTransactionEvents")
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            response(7)
                .pointer("/body/dml")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            4
        );
        assert!(
            response(8)
                .pointer("/body/events")
                .and_then(Value::as_array)
                .is_some_and(|events| events.len() > 4)
        );
    }
}
