use super::{
    Category, EngineInterface, EvaluatedCall, LabeledError, NuvimPlugin, PipelineData,
    PluginCommand, RpcValue, SELECTION_LUA, Signature, SyntaxShape, Type, Value, connect, labeled,
    msgpack_to_nu, non_negative, record, rpc, server_flag,
};
use nu_protocol::Spanned;
use nu_protocol::engine::Closure;

pub(super) struct Transform;

impl PluginCommand for Transform {
    type Plugin = NuvimPlugin;

    fn name(&self) -> &'static str {
        "nuvim transform"
    }

    fn description(&self) -> &'static str {
        "Transform captured text with a closure and reject intervening edits"
    }

    fn signature(&self) -> Signature {
        server_flag(Signature::build(self.name()).category(Category::Plugin))
            .required(
                "closure",
                SyntaxShape::Closure(None),
                "Closure receiving captured text",
            )
            .named("buffer", SyntaxShape::Int, "Target buffer ID", Some('b'))
            .switch("selection", "Capture the last visual selection", None)
            .switch(
                "preview",
                "Return before and after text without applying it",
                None,
            )
            .named("start-row", SyntaxShape::Int, "Zero-based start row", None)
            .named(
                "start-column",
                SyntaxShape::Int,
                "Zero-based start byte column",
                None,
            )
            .named("end-row", SyntaxShape::Int, "Exclusive end row", None)
            .named(
                "end-column",
                SyntaxShape::Int,
                "Exclusive end byte column",
                None,
            )
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
    }

    fn run(
        &self,
        _plugin: &NuvimPlugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: PipelineData,
    ) -> Result<PipelineData, LabeledError> {
        let span = call.head;
        let closure: Spanned<Closure> = call.req(0).map_err(LabeledError::from)?;
        let selection = call.has_flag("selection").map_err(LabeledError::from)?;
        let buffer = call.get_flag::<i64>("buffer").map_err(LabeledError::from)?;
        let mut options = vec![
            (
                RpcValue::from("buffer"),
                RpcValue::from(buffer.unwrap_or(0)),
            ),
            (RpcValue::from("selection"), RpcValue::from(selection)),
        ];
        let mut range = Vec::new();
        for name in ["start-row", "start-column", "end-row", "end-column"] {
            if let Some(value) = call.get_flag::<i64>(name).map_err(LabeledError::from)? {
                range.push(non_negative(value, name, span)?);
            }
        }
        if !range.is_empty() && range.len() != 4 {
            return Err(labeled(
                "range transforms require all four position flags",
                span,
            ));
        }
        if selection && (buffer.is_some() || !range.is_empty()) {
            return Err(labeled(
                "--selection cannot be combined with --buffer or range flags",
                span,
            ));
        }
        if buffer.is_some_and(|id| id < 0) {
            return Err(labeled("buffer ID must be zero or greater", span));
        }
        options.push((
            RpcValue::from("range"),
            RpcValue::Array(range.into_iter().map(RpcValue::from).collect()),
        ));
        let mut client = connect(engine, call)?;
        let capture = format!(
            "local selection = function()\n{SELECTION_LUA}\nend\n{}",
            include_str!("transform-capture.lua")
        );
        let snapshot = rpc(
            client.nvim_exec_lua([
                RpcValue::from(capture),
                RpcValue::Array(vec![RpcValue::Map(options)]),
            ]),
            span,
        )?;
        let captured = msgpack_to_nu(&snapshot, client.server(), span)?;
        let before = captured
            .as_record()
            .map_err(LabeledError::from)?
            .get("text")
            .ok_or_else(|| labeled("captured text is missing", span))?
            .clone();
        let output = engine
            .eval_closure_with_stream(
                &closure,
                vec![before.clone()],
                PipelineData::value(before.clone(), None),
                true,
                false,
            )
            .map_err(LabeledError::from)?
            .into_value(span)
            .map_err(LabeledError::from)?;
        engine.signals().check(&span).map_err(LabeledError::from)?;
        let lines = transformed_lines(&output)?;
        let preview = call.has_flag("preview").map_err(LabeledError::from)?;
        let result = if preview {
            RpcValue::Nil
        } else {
            rpc(
                client.nvim_exec_lua([
                    RpcValue::from(include_str!("transform-apply.lua")),
                    RpcValue::Array(vec![
                        snapshot,
                        RpcValue::Array(lines.iter().cloned().map(RpcValue::from).collect()),
                    ]),
                ]),
                span,
            )?
        };
        Ok(PipelineData::value(
            record(
                [
                    ("server", Value::string(client.server(), span)),
                    ("target", captured),
                    ("before", before),
                    ("after", Value::string(lines.join("\n"), span)),
                    ("applied", Value::bool(!preview, span)),
                    (
                        "changedtick",
                        msgpack_to_nu(&result, client.server(), span)?,
                    ),
                ],
                span,
            ),
            None,
        ))
    }
}

fn transformed_lines(output: &Value) -> Result<Vec<String>, LabeledError> {
    match output {
        Value::String { val, .. } => Ok(val.split('\n').map(str::to_owned).collect()),
        Value::List { vals, .. } => vals
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .map_err(LabeledError::from)
            })
            .collect(),
        Value::Error { error, .. } => Err(LabeledError::from(*error.clone())),
        _ => Err(labeled(
            "transform closure must return text or a list of strings",
            output.span(),
        )),
    }
}
