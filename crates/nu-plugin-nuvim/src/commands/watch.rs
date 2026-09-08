use super::{
    Category, Duration, EngineInterface, EvaluatedCall, LabeledError, NuvimPlugin, NvimHandle,
    PipelineData, PluginCommand, Record, RpcValue, Signature, Span, SyntaxShape, Type, Value,
    connect, labeled, msgpack_to_nu, record, rpc, rpc_array, selected_buffer, server_flag,
};
use nu_protocol::{ListStream, Signals};
use nuvim_protocol::Notification;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TrySendError};
use std::thread::{self, JoinHandle};

struct Watch(&'static str);

pub(super) fn commands() -> Vec<Box<dyn PluginCommand<Plugin = NuvimPlugin>>> {
    [
        "nuvim watch buffer",
        "nuvim watch save",
        "nuvim watch diagnostics",
    ]
    .into_iter()
    .map(|name| Box::new(Watch(name)) as Box<dyn PluginCommand<Plugin = NuvimPlugin>>)
    .collect()
}

impl PluginCommand for Watch {
    type Plugin = NuvimPlugin;
    fn name(&self) -> &'static str {
        self.0
    }
    fn description(&self) -> &'static str {
        "Stream editor events until interrupted or the pipeline closes"
    }
    fn signature(&self) -> Signature {
        server_flag(Signature::build(self.name()).category(Category::Plugin))
            .named(
                "buffer",
                SyntaxShape::Int,
                "Buffer to watch; defaults to current",
                Some('b'),
            )
            .switch(
                "initial",
                "Include the initial text for a buffer watch",
                None,
            )
            .input_output_type(Type::Nothing, Type::List(Type::Any.into()))
    }
    fn run(
        &self,
        _plugin: &NuvimPlugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: PipelineData,
    ) -> Result<PipelineData, LabeledError> {
        let span = call.head;
        let mut client = connect(engine, call)?;
        let buffer = selected_buffer(&mut client, call, span)?;
        let handle = NvimHandle::from_rpc_value(&buffer).map_err(|error| labeled(error, span))?;
        let id = i64::try_from(handle.id).map_err(|error| labeled(error, span))?;
        let info = rpc(client.nvim_get_api_info(), span)?;
        let channel = rpc_array(&info, "API info", span)?
            .first()
            .cloned()
            .ok_or_else(|| labeled("missing RPC channel", span))?;
        let ready = record(
            [
                ("event", Value::string("ready", span)),
                ("server", Value::string(client.server(), span)),
                ("buffer", Value::int(id, span)),
            ],
            span,
        );
        if self.0 == "nuvim watch buffer" {
            let initial = call.has_flag("initial").map_err(LabeledError::from)?;
            let attached = rpc(
                client.nvim_buf_attach([buffer, RpcValue::from(initial), RpcValue::Map(vec![])]),
                span,
            )?;
            if attached != RpcValue::Boolean(true) {
                return Err(labeled("could not attach to buffer", span));
            }
        } else {
            let event = if self.0 == "nuvim watch save" {
                "BufWritePost"
            } else {
                "DiagnosticChanged"
            };
            rpc(
                client.nvim_exec_lua([
                    RpcValue::from(include_str!("watch.lua")),
                    RpcValue::Array(vec![channel, RpcValue::from(id), RpcValue::from(event)]),
                ]),
                span,
            )?;
        }
        let interrupt = client
            .prepare_notifications()
            .map_err(|error| labeled(error, span))?;
        let close_on_overflow = client
            .prepare_notifications()
            .map_err(|error| labeled(error, span))?;
        let server = client.server().to_owned();
        let (sender, receiver) = mpsc::sync_channel(64);
        let worker = thread::spawn(move || {
            loop {
                let notification = client
                    .next_notification()
                    .map_err(|error| labeled(error, span));
                let terminal = !notification
                    .as_ref()
                    .is_ok_and(|event| event.method != "nvim_buf_detach_event");
                let value = notification.and_then(|event| event_value(event, &server, span));
                let detached = value
                    .as_ref()
                    .ok()
                    .and_then(|value| value.as_record().ok())
                    .and_then(|record| record.get("event"))
                    .and_then(|value| value.as_str().ok())
                    == Some("detach");
                let terminal = terminal || value.is_err() || detached;
                match sender.try_send(value) {
                    Ok(()) => (),
                    Err(TrySendError::Disconnected(_)) => break,
                    Err(TrySendError::Full(_)) => {
                        close_on_overflow();
                        drop(client);
                        let _ = sender.send(Err(labeled(
                            "watch overflow: consumer fell behind; start a new watch",
                            span,
                        )));
                        break;
                    }
                }
                if terminal {
                    break;
                }
            }
        });
        let stream = Events {
            ready: Some(ready),
            receiver: Some(receiver),
            interrupt: Some(interrupt),
            worker: Some(worker),
            signals: engine.signals().clone(),
            span,
            ended: false,
        };
        Ok(PipelineData::ListStream(
            ListStream::new(stream, span, engine.signals().clone()),
            None,
        ))
    }
}

struct Events {
    ready: Option<Value>,
    receiver: Option<Receiver<Result<Value, LabeledError>>>,
    interrupt: Option<Box<dyn FnOnce() + Send>>,
    worker: Option<JoinHandle<()>>,
    signals: Signals,
    span: Span,
    ended: bool,
}

impl Iterator for Events {
    type Item = Value;
    fn next(&mut self) -> Option<Value> {
        if self.ended {
            return None;
        }
        if let Some(ready) = self.ready.take() {
            return Some(ready);
        }
        let receiver = self.receiver.as_ref()?;
        loop {
            if self.signals.interrupted() {
                self.ended = true;
                return None;
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(value)) => return Some(value),
                Ok(Err(error)) => {
                    self.ended = true;
                    return Some(Value::error(error.into(), self.span));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.ended = true;
                    return None;
                }
                Err(RecvTimeoutError::Timeout) => (),
            }
        }
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        self.receiver.take();
        if let Some(interrupt) = self.interrupt.take() {
            interrupt();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn event_value(event: Notification, server: &str, span: Span) -> Result<Value, LabeledError> {
    if event.method == "nuvim_event" {
        let data = event
            .arguments
            .into_iter()
            .next()
            .ok_or_else(|| labeled("watch event has no payload", span))?;
        let value = msgpack_to_nu(&data, server, span)?;
        let mut fields = value.as_record().map_err(LabeledError::from)?.clone();
        fields.insert("server", Value::string(server, span));
        return Ok(Value::record(fields, span));
    }
    let names: &[&str] = match event.method.as_str() {
        "nvim_buf_lines_event" => &[
            "buffer",
            "changedtick",
            "firstline",
            "lastline",
            "lines",
            "more",
        ],
        "nvim_buf_changedtick_event" => &["buffer", "changedtick"],
        "nvim_buf_detach_event" => &["buffer"],
        _ => {
            return Err(labeled(
                format!("unexpected watch event {}", event.method),
                span,
            ));
        }
    };
    if names.len() != event.arguments.len() {
        return Err(labeled("invalid watch event arguments", span));
    }
    let mut fields = Record::new();
    fields.push("event", Value::string(event.method, span));
    fields.push("server", Value::string(server, span));
    for (name, value) in names.iter().zip(event.arguments) {
        let value = if *name == "buffer" {
            let id = NvimHandle::from_rpc_value(&value)
                .map_err(|error| labeled(error, span))?
                .id;
            Value::int(
                i64::try_from(id).map_err(|error| labeled(error, span))?,
                span,
            )
        } else {
            msgpack_to_nu(&value, server, span)?
        };
        fields.push(*name, value);
    }
    Ok(Value::record(fields, span))
}

#[cfg(test)]
mod tests {
    use super::Events;
    use nu_protocol::{Signals, Span};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn interruption_ends_an_idle_stream_and_runs_cleanup() {
        let signal = Arc::new(AtomicBool::new(false));
        let cleaned = Arc::new(AtomicBool::new(false));
        let cleanup = Arc::clone(&cleaned);
        let (_sender, receiver) = mpsc::channel();
        let mut stream = Events {
            ready: None,
            receiver: Some(receiver),
            interrupt: Some(Box::new(move || {
                cleanup.store(true, Ordering::SeqCst);
            })),
            worker: None,
            signals: Signals::new(Arc::clone(&signal)),
            span: Span::test_data(),
            ended: false,
        };
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            signal.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        assert!(stream.next().is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(stream);
        assert!(cleaned.load(Ordering::SeqCst));
        trigger.join().expect("signal thread should stop");
    }
}
