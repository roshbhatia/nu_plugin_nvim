use super::{
    Category, EngineInterface, EvaluatedCall, LabeledError, NuvimPlugin, PipelineData,
    PluginCommand, RpcValue, Signature, SyntaxShape, Type, connect, msgpack_to_nu, non_negative,
    rpc, server_flag,
};

struct Query(&'static str);

pub(super) fn commands() -> Vec<Box<dyn PluginCommand<Plugin = NuvimPlugin>>> {
    [
        "nuvim symbols",
        "nuvim references",
        "nuvim definition",
        "nuvim node",
    ]
    .into_iter()
    .map(|name| Box::new(Query(name)) as Box<dyn PluginCommand<Plugin = NuvimPlugin>>)
    .collect()
}

impl PluginCommand for Query {
    type Plugin = NuvimPlugin;
    fn name(&self) -> &'static str {
        self.0
    }
    fn description(&self) -> &'static str {
        "Read semantic editor data as records with zero-based byte positions"
    }
    fn signature(&self) -> Signature {
        server_flag(Signature::build(self.name()).category(Category::Plugin))
            .named("buffer", SyntaxShape::Int, "Buffer to query", Some('b'))
            .named("row", SyntaxShape::Int, "Zero-based row", None)
            .named("column", SyntaxShape::Int, "Zero-based byte column", None)
            .named(
                "timeout",
                SyntaxShape::Int,
                "LSP timeout in milliseconds, 1 to 8000",
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
        let mut options = vec![
            (RpcValue::from("operation"), RpcValue::from(self.0)),
            (RpcValue::from("server"), RpcValue::from(client.server())),
        ];
        for name in ["buffer", "row", "column", "timeout"] {
            if let Some(value) = call.get_flag::<i64>(name).map_err(LabeledError::from)? {
                options.push((
                    RpcValue::from(name),
                    RpcValue::from(non_negative(value, name, span)?),
                ));
            }
        }
        let result = rpc(
            client.nvim_exec_lua([
                RpcValue::from(include_str!("query.lua")),
                RpcValue::Array(vec![RpcValue::Map(options)]),
            ]),
            span,
        )?;
        Ok(PipelineData::value(
            msgpack_to_nu(&result, client.server(), span)?,
            None,
        ))
    }
}
