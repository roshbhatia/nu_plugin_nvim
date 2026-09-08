use super::{
    Category, EngineInterface, EvaluatedCall, LabeledError, NuvimPlugin, PipelineData,
    PluginCommand, RpcValue, Signature, SyntaxShape, Type, Value, connect, msgpack_to_nu,
    nu_to_msgpack, rpc, server_flag,
};

pub(super) fn flags(signature: Signature) -> Signature {
    signature
        .named("id", SyntaxShape::Int, "Existing list ID", None)
        .named(
            "window",
            SyntaxShape::Int,
            "Use this window's location list; 0 means current",
            None,
        )
}

pub(super) struct History;

pub(super) fn set_signature(signature: Signature) -> Signature {
    flags(server_flag(signature))
        .named(
            "action",
            SyntaxShape::String,
            "new, append, or replace (default)",
            None,
        )
        .named("context", SyntaxShape::Any, "List metadata", None)
        .named(
            "title",
            SyntaxShape::String,
            "Quickfix list title",
            Some('t'),
        )
        .input_output_type(Type::Any, Type::Record(vec![].into()))
}

impl PluginCommand for History {
    type Plugin = NuvimPlugin;
    fn name(&self) -> &'static str {
        "nuvim quickfix history"
    }
    fn description(&self) -> &'static str {
        "List quickfix or location-list identities and metadata"
    }
    fn signature(&self) -> Signature {
        server_flag(Signature::build(self.name()).category(Category::Plugin))
            .named(
                "window",
                SyntaxShape::Int,
                "Window owning the location-list history",
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
        Ok(PipelineData::value(
            run(engine, call, "history", RpcValue::Nil)?,
            None,
        ))
    }
}

pub(super) fn run(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    operation: &str,
    items: RpcValue,
) -> Result<Value, LabeledError> {
    let mut client = connect(engine, call)?;
    let span = call.head;
    let mut options = vec![
        (RpcValue::from("operation"), RpcValue::from(operation)),
        (RpcValue::from("server"), RpcValue::from(client.server())),
        (RpcValue::from("items"), items),
    ];
    for name in ["id", "window", "action", "title", "context"] {
        if let Some(value) = call.get_flag::<Value>(name).map_err(LabeledError::from)? {
            options.push((
                RpcValue::from(name),
                nu_to_msgpack(&value, client.server())?,
            ));
        }
    }
    options.push((
        RpcValue::from("details"),
        RpcValue::from(call.has_flag("details").map_err(LabeledError::from)?),
    ));
    let result = rpc(
        client.nvim_exec_lua([
            RpcValue::from(include_str!("quickfix.lua")),
            RpcValue::Array(vec![RpcValue::Map(options)]),
        ]),
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}
