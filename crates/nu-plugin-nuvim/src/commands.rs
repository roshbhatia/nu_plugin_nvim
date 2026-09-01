use std::path::PathBuf;

use nu_plugin::{EngineInterface, EvaluatedCall, PluginCommand};
use nu_protocol::{
    Category, LabeledError, PipelineData, Record, Signature, Span, SyntaxShape, Type, Value,
};
use nuvim_protocol::{
    ApiMetadata, HandleKind, NvimHandle, QuickfixItem, RpcClient, discover_server,
};
use rmpv::Value as RpcValue;

use crate::NuvimPlugin;
use crate::value::{msgpack_to_nu, nu_to_msgpack};

#[derive(Clone, Copy)]
enum CommandKind {
    Root,
    Context,
    Buffers,
    Text,
    Selection,
    Open,
    Replace,
    Diagnostics,
    QuickfixGet,
    QuickfixSet,
    QuickfixOpen,
    Scratch,
    Call,
    Lua,
}

struct NuvimCommand(CommandKind);

pub fn all() -> Vec<Box<dyn PluginCommand<Plugin = NuvimPlugin>>> {
    use CommandKind::{
        Buffers, Call, Context, Diagnostics, Lua, Open, QuickfixGet, QuickfixOpen, QuickfixSet,
        Replace, Root, Scratch, Selection, Text,
    };
    [
        Root,
        Context,
        Buffers,
        Text,
        Selection,
        Open,
        Replace,
        Diagnostics,
        QuickfixGet,
        QuickfixSet,
        QuickfixOpen,
        Scratch,
        Call,
        Lua,
    ]
    .into_iter()
    .map(|kind| Box::new(NuvimCommand(kind)) as Box<dyn PluginCommand<Plugin = NuvimPlugin>>)
    .collect()
}

impl PluginCommand for NuvimCommand {
    type Plugin = NuvimPlugin;

    fn name(&self) -> &str {
        match self.0 {
            CommandKind::Root => "nuvim",
            CommandKind::Context => "nuvim context",
            CommandKind::Buffers => "nuvim buffers",
            CommandKind::Text => "nuvim text",
            CommandKind::Selection => "nuvim selection",
            CommandKind::Open => "nuvim open",
            CommandKind::Replace => "nuvim replace",
            CommandKind::Diagnostics => "nuvim diagnostics",
            CommandKind::QuickfixGet => "nuvim quickfix get",
            CommandKind::QuickfixSet => "nuvim quickfix set",
            CommandKind::QuickfixOpen => "nuvim quickfix open",
            CommandKind::Scratch => "nuvim scratch",
            CommandKind::Call => "nuvim call",
            CommandKind::Lua => "nuvim lua",
        }
    }

    fn signature(&self) -> Signature {
        let signature = Signature::build(self.name()).category(Category::Plugin);
        match self.0 {
            CommandKind::Root => {
                signature.input_output_type(Type::Nothing, Type::List(Type::String.into()))
            }
            CommandKind::Context | CommandKind::Selection => {
                server_flag(signature).input_output_type(Type::Nothing, Type::Record(vec![].into()))
            }
            CommandKind::Buffers | CommandKind::Diagnostics | CommandKind::QuickfixGet => {
                server_flag(signature)
                    .input_output_type(Type::Nothing, Type::List(Type::Any.into()))
            }
            CommandKind::Text => server_flag(signature)
                .named(
                    "buffer",
                    SyntaxShape::Int,
                    "Buffer ID; defaults to the current buffer",
                    Some('b'),
                )
                .named("start", SyntaxShape::Int, "First zero-based row", None)
                .named(
                    "end",
                    SyntaxShape::Int,
                    "Exclusive zero-based row; defaults to the buffer end",
                    None,
                )
                .input_output_type(Type::Nothing, Type::Record(vec![].into())),
            CommandKind::Open => server_flag(signature)
                .rest(
                    "paths",
                    SyntaxShape::Filepath,
                    "Paths to open after pipeline input",
                )
                .input_output_types(vec![
                    (Type::Nothing, Type::List(Type::Any.into())),
                    (Type::Any, Type::List(Type::Any.into())),
                ]),
            CommandKind::Replace => server_flag(signature)
                .switch("selection", "Replace the last visual selection", None)
                .named(
                    "buffer",
                    SyntaxShape::Int,
                    "Buffer ID; defaults to the current buffer",
                    Some('b'),
                )
                .input_output_type(Type::Any, Type::Record(vec![].into())),
            CommandKind::QuickfixSet => server_flag(signature)
                .named(
                    "title",
                    SyntaxShape::String,
                    "Quickfix list title",
                    Some('t'),
                )
                .input_output_type(Type::Any, Type::Record(vec![].into())),
            CommandKind::QuickfixOpen => server_flag(signature)
                .named("height", SyntaxShape::Int, "Quickfix window height", None)
                .input_output_type(Type::Any, Type::Nothing),
            CommandKind::Scratch => server_flag(signature)
                .named(
                    "name",
                    SyntaxShape::String,
                    "Scratch buffer name",
                    Some('n'),
                )
                .named(
                    "filetype",
                    SyntaxShape::String,
                    "Scratch buffer filetype",
                    Some('f'),
                )
                .input_output_type(Type::Any, Type::Record(vec![].into())),
            CommandKind::Call => server_flag(signature)
                .required("method", SyntaxShape::String, "Neovim API method name")
                .rest(
                    "arguments",
                    SyntaxShape::Any,
                    "MessagePack-compatible method arguments",
                )
                .input_output_type(Type::Any, Type::Any),
            CommandKind::Lua => server_flag(signature)
                .required(
                    "code",
                    SyntaxShape::String,
                    "Lua source evaluated by nvim_exec_lua",
                )
                .rest(
                    "arguments",
                    SyntaxShape::Any,
                    "MessagePack-compatible Lua arguments",
                )
                .input_output_type(Type::Any, Type::Any),
        }
    }

    fn description(&self) -> &str {
        match self.0 {
            CommandKind::Root => "Use Neovim as a structured Nushell data source and sink",
            CommandKind::Context => "Get current Neovim context as a record",
            CommandKind::Buffers => "List Neovim buffers as records",
            CommandKind::Text => "Read buffer text and its zero-based row range",
            CommandKind::Selection => "Read the last visual selection as structured text",
            CommandKind::Open => "Open paths from pipeline input or arguments",
            CommandKind::Replace => "Replace a buffer or visual selection with pipeline input",
            CommandKind::Diagnostics => "List Neovim diagnostics as records",
            CommandKind::QuickfixGet => "Get quickfix items with zero-based positions",
            CommandKind::QuickfixSet => "Replace the quickfix list from pipeline records",
            CommandKind::QuickfixOpen => "Open the Neovim quickfix window",
            CommandKind::Scratch => "Open pipeline input in a scratch buffer",
            CommandKind::Call => "Call a raw Neovim API method after metadata validation",
            CommandKind::Lua => "Evaluate Lua in Neovim with MessagePack-compatible arguments",
        }
    }

    fn extra_description(&self) -> &'static str {
        "All public rows and columns are zero-based. Columns are UTF-8 byte offsets, matching Neovim's API."
    }

    fn run(
        &self,
        _plugin: &NuvimPlugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        input: PipelineData,
    ) -> Result<PipelineData, LabeledError> {
        let output = match self.0 {
            CommandKind::Root => root(call.head),
            CommandKind::Context => context(call)?,
            CommandKind::Buffers => buffers(call)?,
            CommandKind::Text => text(call)?,
            CommandKind::Selection => selection(call)?,
            CommandKind::Open => open(call, input)?,
            CommandKind::Replace => replace(call, input)?,
            CommandKind::Diagnostics => diagnostics(call)?,
            CommandKind::QuickfixGet => quickfix_get(call)?,
            CommandKind::QuickfixSet => quickfix_set(call, input)?,
            CommandKind::QuickfixOpen => return quickfix_open(call),
            CommandKind::Scratch => scratch(engine, call, input)?,
            CommandKind::Call => raw_call(call, input)?,
            CommandKind::Lua => lua(call, input)?,
        };
        Ok(PipelineData::value(output, None))
    }
}

fn server_flag(signature: Signature) -> Signature {
    signature.named(
        "server",
        SyntaxShape::String,
        "Neovim socket path or TCP address; overrides $NVIM",
        Some('s'),
    )
}

fn root(span: Span) -> Value {
    Value::list(
        [
            "context",
            "buffers",
            "text",
            "selection",
            "open",
            "replace",
            "diagnostics",
            "quickfix get",
            "quickfix set",
            "quickfix open",
            "scratch",
            "call",
            "lua",
        ]
        .into_iter()
        .map(|name| Value::string(name, span))
        .collect(),
        span,
    )
}

fn connect(call: &EvaluatedCall) -> Result<RpcClient, LabeledError> {
    let override_value = call
        .get_flag::<String>("server")
        .map_err(LabeledError::from)?;
    let address =
        discover_server(override_value.as_deref()).map_err(|error| labeled(error, call.head))?;
    RpcClient::connect(&address).map_err(|error| labeled(error, call.head))
}

fn context(call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(call)?;
    let buffer = rpc(&mut client, "nvim_get_current_buf", vec![], span)?;
    let window = rpc(&mut client, "nvim_get_current_win", vec![], span)?;
    let tab = rpc(&mut client, "nvim_get_current_tabpage", vec![], span)?;
    let mode = rpc(&mut client, "nvim_get_mode", vec![], span)?;
    let cursor = rpc(
        &mut client,
        "nvim_win_get_cursor",
        vec![window.clone()],
        span,
    )?;
    let cwd = rpc(
        &mut client,
        "nvim_call_function",
        vec![RpcValue::from("getcwd"), RpcValue::Array(vec![])],
        span,
    )?;
    let server = client.server().to_owned();
    Ok(record(
        [
            ("server", Value::string(&server, span)),
            (
                "mode",
                Value::string(rpc_string_field(&mode, "mode")?, span),
            ),
            ("buffer", buffer_record(&mut client, &buffer, span)?),
            ("window", window_record(&mut client, &window, span)?),
            ("tab", handle_summary(&tab, &server, span)?),
            ("cursor", cursor_record(&cursor, span)?),
            (
                "cwd",
                Value::string(rpc_string(&cwd, "working directory")?, span),
            ),
        ],
        span,
    ))
}

fn buffers(call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(call)?;
    let listed = rpc(&mut client, "nvim_list_bufs", vec![], span)?;
    let listed = rpc_array(&listed, "nvim_list_bufs result", span)?;
    let rows = listed
        .iter()
        .map(|buffer| buffer_record(&mut client, buffer, span))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::list(rows, span))
}

fn text(call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(call)?;
    let buffer = selected_buffer(&mut client, call, span)?;
    let start = call
        .get_flag::<i64>("start")
        .map_err(LabeledError::from)?
        .unwrap_or(0);
    let end = call
        .get_flag::<i64>("end")
        .map_err(LabeledError::from)?
        .unwrap_or(-1);
    if start < 0 || end < -1 {
        return Err(labeled(
            "row ranges must be zero or greater; --end may be -1",
            span,
        ));
    }
    text_for_buffer(&mut client, &buffer, start, end, span)
}

fn text_for_buffer(
    client: &mut RpcClient,
    buffer: &RpcValue,
    start: i64,
    end: i64,
    span: Span,
) -> Result<Value, LabeledError> {
    let result = rpc(
        client,
        "nvim_buf_get_lines",
        vec![
            buffer.clone(),
            RpcValue::from(start),
            RpcValue::from(end),
            RpcValue::Boolean(true),
        ],
        span,
    )?;
    let raw_lines = rpc_array(&result, "buffer lines", span)?;
    let lines = raw_lines
        .iter()
        .map(|line| rpc_string(line, "buffer line").map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let actual_end = start + i64::try_from(lines.len()).map_err(|error| labeled(error, span))?;
    let server = client.server().to_owned();
    Ok(record(
        [
            ("buffer", handle_summary(buffer, &server, span)?),
            ("start", Value::int(start, span)),
            ("end", Value::int(actual_end, span)),
            (
                "lines",
                Value::list(
                    lines.iter().map(|line| Value::string(line, span)).collect(),
                    span,
                ),
            ),
            ("text", Value::string(lines.join("\n"), span)),
        ],
        span,
    ))
}

fn selection(call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(call)?;
    let result = rpc(
        &mut client,
        "nvim_exec_lua",
        vec![RpcValue::from(SELECTION_LUA), RpcValue::Array(vec![])],
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn open(call: &EvaluatedCall, input: PipelineData) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut paths = paths_from_input(input.into_value(span).map_err(LabeledError::from)?, span)?;
    paths.extend(call.rest::<PathBuf>(0).map_err(LabeledError::from)?);
    if paths.is_empty() {
        return Err(labeled(
            "nuvim open needs path arguments or pipeline input",
            span,
        ));
    }
    let mut client = connect(call)?;
    let mut opened = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.to_string_lossy().into_owned();
        let id = rpc(
            &mut client,
            "nvim_call_function",
            vec![
                RpcValue::from("bufadd"),
                RpcValue::Array(vec![RpcValue::from(path)]),
            ],
            span,
        )?
        .as_u64()
        .ok_or_else(|| labeled("bufadd returned a non-integer buffer ID", span))?;
        rpc(
            &mut client,
            "nvim_call_function",
            vec![
                RpcValue::from("bufload"),
                RpcValue::Array(vec![RpcValue::from(id)]),
            ],
            span,
        )?;
        let buffer = NvimHandle::new(HandleKind::Buffer, id)
            .to_rpc_value()
            .map_err(|error| labeled(error, span))?;
        if opened.is_empty() {
            rpc(
                &mut client,
                "nvim_set_current_buf",
                vec![buffer.clone()],
                span,
            )?;
        }
        opened.push(buffer_record(&mut client, &buffer, span)?);
    }
    Ok(Value::list(opened, span))
}

fn replace(call: &EvaluatedCall, input: PipelineData) -> Result<Value, LabeledError> {
    let span = call.head;
    let value = input.into_value(span).map_err(LabeledError::from)?;
    let lines = value_to_lines(&value, None, span)?;
    let rpc_lines = RpcValue::Array(
        lines
            .iter()
            .map(|line| RpcValue::from(line.clone()))
            .collect(),
    );
    let mut client = connect(call)?;
    let selection = call.has_flag("selection").map_err(LabeledError::from)?;
    let buffer = selected_buffer(&mut client, call, span)?;
    if selection {
        rpc(
            &mut client,
            "nvim_exec_lua",
            vec![
                RpcValue::from(REPLACE_SELECTION_LUA),
                RpcValue::Array(vec![rpc_lines]),
            ],
            span,
        )?;
    } else {
        rpc(
            &mut client,
            "nvim_buf_set_lines",
            vec![
                buffer.clone(),
                RpcValue::from(0),
                RpcValue::from(-1),
                RpcValue::Boolean(true),
                rpc_lines,
            ],
            span,
        )?;
    }
    text_for_buffer(&mut client, &buffer, 0, -1, span)
}

fn diagnostics(call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(call)?;
    let result = rpc(
        &mut client,
        "nvim_exec_lua",
        vec![RpcValue::from(DIAGNOSTICS_LUA), RpcValue::Array(vec![])],
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn quickfix_get(call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(call)?;
    let result = rpc(
        &mut client,
        "nvim_exec_lua",
        vec![RpcValue::from(QUICKFIX_GET_LUA), RpcValue::Array(vec![])],
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn quickfix_set(call: &EvaluatedCall, input: PipelineData) -> Result<Value, LabeledError> {
    let span = call.head;
    let value = input.into_value(span).map_err(LabeledError::from)?;
    let values: Vec<&Value> = match &value {
        Value::List { vals, .. } => vals.iter().collect(),
        Value::Record { .. } => vec![&value],
        Value::Nothing { .. } => {
            return Err(labeled(
                "nuvim quickfix set needs record pipeline input",
                span,
            ));
        }
        _ => {
            return Err(labeled(
                "quickfix input must be a record or list of records",
                value.span(),
            ));
        }
    };
    let items = values
        .into_iter()
        .map(quickfix_from_nu)
        .map(|item| item.and_then(|item| item.to_rpc_value().map_err(|error| labeled(error, span))))
        .collect::<Result<Vec<_>, _>>()?;
    let count = i64::try_from(items.len()).map_err(|error| labeled(error, span))?;
    let title = call
        .get_flag::<String>("title")
        .map_err(LabeledError::from)?
        .unwrap_or_else(|| "Nuvim".into());
    let mut client = connect(call)?;
    rpc(
        &mut client,
        "nvim_call_function",
        vec![
            RpcValue::from("setqflist"),
            RpcValue::Array(vec![
                RpcValue::Array(vec![]),
                RpcValue::from("r"),
                RpcValue::Map(vec![
                    (RpcValue::from("title"), RpcValue::from(title.clone())),
                    (RpcValue::from("items"), RpcValue::Array(items)),
                ]),
            ]),
        ],
        span,
    )?;
    Ok(record(
        [
            ("count", Value::int(count, span)),
            ("title", Value::string(title, span)),
        ],
        span,
    ))
}

fn quickfix_open(call: &EvaluatedCall) -> Result<PipelineData, LabeledError> {
    let span = call.head;
    let height = call.get_flag::<i64>("height").map_err(LabeledError::from)?;
    if height.is_some_and(|height| height <= 0) {
        return Err(labeled("quickfix height must be greater than zero", span));
    }
    let command = height.map_or_else(|| "copen".into(), |height| format!("{height}copen"));
    let mut client = connect(call)?;
    rpc(
        &mut client,
        "nvim_command",
        vec![RpcValue::from(command)],
        span,
    )?;
    Ok(PipelineData::empty())
}

fn scratch(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
    let span = call.head;
    let value = input.into_value(span).map_err(LabeledError::from)?;
    let config = engine.get_config().map_err(LabeledError::from)?;
    let lines = value_to_lines(&value, Some(&config), span)?;
    let mut client = connect(call)?;
    let buffer = rpc(
        &mut client,
        "nvim_create_buf",
        vec![RpcValue::Boolean(false), RpcValue::Boolean(true)],
        span,
    )?;
    rpc(
        &mut client,
        "nvim_buf_set_lines",
        vec![
            buffer.clone(),
            RpcValue::from(0),
            RpcValue::from(-1),
            RpcValue::Boolean(true),
            RpcValue::Array(lines.into_iter().map(RpcValue::from).collect()),
        ],
        span,
    )?;
    if let Some(name) = call
        .get_flag::<String>("name")
        .map_err(LabeledError::from)?
    {
        rpc(
            &mut client,
            "nvim_buf_set_name",
            vec![buffer.clone(), RpcValue::from(name)],
            span,
        )?;
    }
    if let Some(filetype) = call
        .get_flag::<String>("filetype")
        .map_err(LabeledError::from)?
    {
        set_buffer_option(
            &mut client,
            &buffer,
            "filetype",
            RpcValue::from(filetype),
            span,
        )?;
    }
    rpc(
        &mut client,
        "nvim_set_current_buf",
        vec![buffer.clone()],
        span,
    )?;
    buffer_record(&mut client, &buffer, span)
}

fn raw_call(call: &EvaluatedCall, input: PipelineData) -> Result<Value, LabeledError> {
    let span = call.head;
    let method = call.req::<String>(0).map_err(LabeledError::from)?;
    let mut arguments = call.rest::<Value>(1).map_err(LabeledError::from)?;
    append_pipeline_arguments(
        &mut arguments,
        input.into_value(span).map_err(LabeledError::from)?,
    );
    let arguments = arguments
        .iter()
        .map(nu_to_msgpack)
        .collect::<Result<Vec<_>, _>>()?;
    let mut client = connect(call)?;
    let api_info = rpc(&mut client, "nvim_get_api_info", vec![], span)?;
    let metadata = ApiMetadata::from_api_info(&api_info).map_err(|error| labeled(error, span))?;
    let function = metadata.function(&method).ok_or_else(|| {
        labeled(
            format!("Neovim API metadata does not contain method {method}"),
            call.positional.first().map_or(span, Value::span),
        )
    })?;
    if arguments.len() != function.parameters.len() {
        return Err(labeled(
            format!(
                "{method} needs {} arguments, received {}",
                function.parameters.len(),
                arguments.len()
            ),
            span,
        ));
    }
    let result = rpc(&mut client, &method, arguments, span)?;
    msgpack_to_nu(&result, client.server(), span)
}

fn lua(call: &EvaluatedCall, input: PipelineData) -> Result<Value, LabeledError> {
    let span = call.head;
    let code = call.req::<String>(0).map_err(LabeledError::from)?;
    let mut arguments = call.rest::<Value>(1).map_err(LabeledError::from)?;
    append_pipeline_arguments(
        &mut arguments,
        input.into_value(span).map_err(LabeledError::from)?,
    );
    let arguments = arguments
        .iter()
        .map(nu_to_msgpack)
        .collect::<Result<Vec<_>, _>>()?;
    let mut client = connect(call)?;
    let result = rpc(
        &mut client,
        "nvim_exec_lua",
        vec![RpcValue::from(code), RpcValue::Array(arguments)],
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn rpc(
    client: &mut RpcClient,
    method: &str,
    arguments: Vec<RpcValue>,
    span: Span,
) -> Result<RpcValue, LabeledError> {
    client
        .call(method, arguments)
        .map_err(|error| labeled(error, span))
}

fn buffer_record(
    client: &mut RpcClient,
    buffer: &RpcValue,
    span: Span,
) -> Result<Value, LabeledError> {
    let name = rpc(client, "nvim_buf_get_name", vec![buffer.clone()], span)?;
    let changedtick = rpc(
        client,
        "nvim_buf_get_changedtick",
        vec![buffer.clone()],
        span,
    )?;
    let loaded = rpc(client, "nvim_buf_is_loaded", vec![buffer.clone()], span)?;
    let filetype = get_buffer_option(client, buffer, "filetype", span)?;
    let modified = get_buffer_option(client, buffer, "modified", span)?;
    let handle = NvimHandle::from_rpc_value(buffer).map_err(|error| labeled(error, span))?;
    let id = i64::try_from(handle.id).map_err(|error| labeled(error, span))?;
    Ok(record(
        [
            ("id", Value::int(id, span)),
            (
                "path",
                Value::string(rpc_string(&name, "buffer name")?, span),
            ),
            (
                "filetype",
                Value::string(rpc_string(&filetype, "buffer filetype")?, span),
            ),
            (
                "modified",
                Value::bool(rpc_bool(&modified, "buffer modified state", span)?, span),
            ),
            (
                "changedtick",
                Value::int(rpc_i64(&changedtick, "buffer changedtick", span)?, span),
            ),
            (
                "loaded",
                Value::bool(rpc_bool(&loaded, "buffer loaded state", span)?, span),
            ),
            ("server", Value::string(client.server(), span)),
        ],
        span,
    ))
}

fn window_record(
    client: &mut RpcClient,
    window: &RpcValue,
    span: Span,
) -> Result<Value, LabeledError> {
    let width = rpc(client, "nvim_win_get_width", vec![window.clone()], span)?;
    let height = rpc(client, "nvim_win_get_height", vec![window.clone()], span)?;
    let handle = NvimHandle::from_rpc_value(window).map_err(|error| labeled(error, span))?;
    Ok(record(
        [
            (
                "id",
                Value::int(
                    i64::try_from(handle.id).map_err(|error| labeled(error, span))?,
                    span,
                ),
            ),
            (
                "width",
                Value::int(rpc_i64(&width, "window width", span)?, span),
            ),
            (
                "height",
                Value::int(rpc_i64(&height, "window height", span)?, span),
            ),
            ("server", Value::string(client.server(), span)),
        ],
        span,
    ))
}

fn handle_summary(handle: &RpcValue, server: &str, span: Span) -> Result<Value, LabeledError> {
    let handle = NvimHandle::from_rpc_value(handle).map_err(|error| labeled(error, span))?;
    Ok(record(
        [
            ("type", Value::string(handle.kind.as_str(), span)),
            (
                "id",
                Value::int(
                    i64::try_from(handle.id).map_err(|error| labeled(error, span))?,
                    span,
                ),
            ),
            ("server", Value::string(server, span)),
        ],
        span,
    ))
}

fn cursor_record(cursor: &RpcValue, span: Span) -> Result<Value, LabeledError> {
    let values = rpc_array(cursor, "window cursor", span)?;
    let row = values
        .first()
        .and_then(RpcValue::as_i64)
        .ok_or_else(|| labeled("cursor row is not an integer", span))?;
    let column = values
        .get(1)
        .and_then(RpcValue::as_i64)
        .ok_or_else(|| labeled("cursor column is not an integer", span))?;
    Ok(record(
        [
            ("row", Value::int(row.saturating_sub(1), span)),
            ("column", Value::int(column, span)),
        ],
        span,
    ))
}

fn selected_buffer(
    client: &mut RpcClient,
    call: &EvaluatedCall,
    span: Span,
) -> Result<RpcValue, LabeledError> {
    if let Some(id) = call.get_flag::<i64>("buffer").map_err(LabeledError::from)? {
        let id = u64::try_from(id).map_err(|_| labeled("buffer ID must be non-negative", span))?;
        return NvimHandle::new(HandleKind::Buffer, id)
            .to_rpc_value()
            .map_err(|error| labeled(error, span));
    }
    rpc(client, "nvim_get_current_buf", vec![], span)
}

fn get_buffer_option(
    client: &mut RpcClient,
    buffer: &RpcValue,
    name: &str,
    span: Span,
) -> Result<RpcValue, LabeledError> {
    rpc(
        client,
        "nvim_get_option_value",
        vec![
            RpcValue::from(name),
            RpcValue::Map(vec![(RpcValue::from("buf"), buffer.clone())]),
        ],
        span,
    )
}

fn set_buffer_option(
    client: &mut RpcClient,
    buffer: &RpcValue,
    name: &str,
    value: RpcValue,
    span: Span,
) -> Result<(), LabeledError> {
    rpc(
        client,
        "nvim_set_option_value",
        vec![
            RpcValue::from(name),
            value,
            RpcValue::Map(vec![(RpcValue::from("buf"), buffer.clone())]),
        ],
        span,
    )?;
    Ok(())
}

fn quickfix_from_nu(value: &Value) -> Result<QuickfixItem, LabeledError> {
    let span = value.span();
    let record = value
        .as_record()
        .map_err(|_| labeled("quickfix item must be a record", span))?;
    Ok(QuickfixItem {
        path: optional_string(record, "path")?,
        row: optional_position(record, "row")?,
        column: optional_position(record, "column")?,
        end_row: optional_position(record, "end_row")?,
        end_column: optional_position(record, "end_column")?,
        text: optional_string(record, "text")?,
        item_type: optional_string(record, "type")?,
    })
}

fn optional_string(record: &Record, name: &str) -> Result<Option<String>, LabeledError> {
    let Some(value) = nu_field(record, name) else {
        return Ok(None);
    };
    if value.is_nothing() {
        return Ok(None);
    }
    value
        .as_str()
        .map(|value| Some(value.to_owned()))
        .map_err(|_| {
            labeled(
                format!("quickfix {name} must be a string or null"),
                value.span(),
            )
        })
}

fn optional_position(record: &Record, name: &str) -> Result<Option<u64>, LabeledError> {
    let Some(value) = nu_field(record, name) else {
        return Ok(None);
    };
    if value.is_nothing() {
        return Ok(None);
    }
    let position = value.as_int().map_err(|_| {
        labeled(
            format!("quickfix {name} must be an integer or null"),
            value.span(),
        )
    })?;
    u64::try_from(position).map(Some).map_err(|_| {
        labeled(
            format!("quickfix {name} must be zero or greater"),
            value.span(),
        )
    })
}

fn paths_from_input(value: Value, span: Span) -> Result<Vec<PathBuf>, LabeledError> {
    match value {
        Value::Nothing { .. } => Ok(vec![]),
        Value::String { val, .. } | Value::Glob { val, .. } => Ok(vec![PathBuf::from(val)]),
        Value::List { vals, .. } => vals.iter().map(path_from_value).collect(),
        Value::Record { val, .. } => nu_field(&val, "path")
            .ok_or_else(|| labeled("path record has no path field", span))
            .and_then(path_from_value)
            .map(|path| vec![path]),
        _ => Err(labeled(
            "nuvim open input must contain paths or path records",
            span,
        )),
    }
}

fn path_from_value(value: &Value) -> Result<PathBuf, LabeledError> {
    match value {
        Value::String { val, .. } | Value::Glob { val, .. } => Ok(PathBuf::from(val)),
        Value::Record { val, .. } => nu_field(val, "path")
            .ok_or_else(|| labeled("path record has no path field", value.span()))
            .and_then(path_from_value),
        _ => Err(labeled(
            "path input must be a string or record with a path field",
            value.span(),
        )),
    }
}

fn value_to_lines(
    value: &Value,
    config: Option<&nu_protocol::Config>,
    span: Span,
) -> Result<Vec<String>, LabeledError> {
    match value {
        Value::String { val, .. } | Value::Glob { val, .. } => Ok(split_lines(val)),
        Value::Binary { val, .. } => std::str::from_utf8(val)
            .map(split_lines)
            .map_err(|error| labeled(format!("input is not UTF-8 text: {error}"), value.span())),
        Value::List { vals, .. } if vals.iter().all(|value| value.as_str().is_ok()) => Ok(vals
            .iter()
            .map(|value| value.as_str().expect("list values were checked").to_owned())
            .collect()),
        Value::Nothing { .. } => Ok(vec![String::new()]),
        _ => config.map_or_else(
            || {
                Err(labeled(
                    "structured input is supported by nuvim scratch only",
                    span,
                ))
            },
            |config| Ok(split_lines(&value.to_expanded_string("\n", config))),
        ),
    }
}

fn split_lines(text: &str) -> Vec<String> {
    let mut lines = text.split('\n').map(str::to_owned).collect::<Vec<_>>();
    if text.ends_with('\n') {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn append_pipeline_arguments(arguments: &mut Vec<Value>, input: Value) {
    if input.is_nothing() {
        return;
    }
    if arguments.is_empty() {
        match input {
            Value::List { vals, .. } => arguments.extend(vals.iter().cloned()),
            other => arguments.push(other),
        }
    } else {
        arguments.push(input);
    }
}

fn rpc_array<'a>(
    value: &'a RpcValue,
    context: &str,
    span: Span,
) -> Result<&'a [RpcValue], LabeledError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| labeled(format!("{context} is not an array"), span))
}

fn rpc_string<'a>(value: &'a RpcValue, context: &str) -> Result<&'a str, LabeledError> {
    value
        .as_str()
        .ok_or_else(|| labeled(format!("{context} is not a string"), Span::unknown()))
}

fn rpc_string_field<'a>(value: &'a RpcValue, name: &str) -> Result<&'a str, LabeledError> {
    let map = value
        .as_map()
        .ok_or_else(|| labeled("RPC result is not a map", Span::unknown()))?;
    map.iter()
        .find_map(|(key, value)| (key.as_str() == Some(name)).then_some(value))
        .and_then(RpcValue::as_str)
        .ok_or_else(|| {
            labeled(
                format!("RPC result has no string {name} field"),
                Span::unknown(),
            )
        })
}

fn rpc_i64(value: &RpcValue, context: &str, span: Span) -> Result<i64, LabeledError> {
    value
        .as_i64()
        .ok_or_else(|| labeled(format!("{context} is not an integer"), span))
}

fn rpc_bool(value: &RpcValue, context: &str, span: Span) -> Result<bool, LabeledError> {
    value
        .as_bool()
        .ok_or_else(|| labeled(format!("{context} is not a boolean"), span))
}

fn nu_field<'a>(record: &'a Record, name: &str) -> Option<&'a Value> {
    record
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn record<const N: usize>(fields: [(&str, Value); N], span: Span) -> Value {
    Value::record(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
        span,
    )
}

fn labeled(error: impl std::fmt::Display, span: Span) -> LabeledError {
    LabeledError::new(error.to_string())
        .with_code("nuvim::error")
        .with_label("Nuvim command failed here", span)
}

const SELECTION_LUA: &str = r#"
local mode = vim.fn.visualmode()
if mode == "\022" then error("blockwise selections are not supported in Nuvim 0.1") end
local first = vim.api.nvim_buf_get_mark(0, "<")
local last = vim.api.nvim_buf_get_mark(0, ">")
if first[1] == 0 or last[1] == 0 then error("no visual selection is available") end
local start_row, start_col = first[1] - 1, first[2]
local end_row, end_col = last[1] - 1, last[2] + 1
if mode == "V" then
  start_col = 0
  end_col = #vim.api.nvim_buf_get_lines(0, end_row, end_row + 1, true)[1]
end
local lines = vim.api.nvim_buf_get_text(0, start_row, start_col, end_row, end_col, {})
return {
  buffer = vim.api.nvim_get_current_buf(),
  mode = mode,
  start = { row = start_row, column = start_col },
  ["end"] = { row = end_row, column = end_col },
  lines = lines,
  text = table.concat(lines, "\n"),
}
"#;

const REPLACE_SELECTION_LUA: &str = r#"
local replacement = ...
local mode = vim.fn.visualmode()
if mode == "\022" then error("blockwise selections are not supported in Nuvim 0.1") end
local first = vim.api.nvim_buf_get_mark(0, "<")
local last = vim.api.nvim_buf_get_mark(0, ">")
if first[1] == 0 or last[1] == 0 then error("no visual selection is available") end
local start_row, start_col = first[1] - 1, first[2]
local end_row, end_col = last[1] - 1, last[2] + 1
if mode == "V" then
  vim.api.nvim_buf_set_lines(0, start_row, end_row + 1, true, replacement)
else
  vim.api.nvim_buf_set_text(0, start_row, start_col, end_row, end_col, replacement)
end
return true
"#;

const DIAGNOSTICS_LUA: &str = r"
local output = {}
for _, diagnostic in ipairs(vim.diagnostic.get(nil)) do
  local severity = vim.diagnostic.severity[diagnostic.severity] or tostring(diagnostic.severity)
  table.insert(output, {
    buffer = diagnostic.bufnr,
    path = vim.api.nvim_buf_get_name(diagnostic.bufnr),
    row = diagnostic.lnum,
    column = diagnostic.col,
    end_row = diagnostic.end_lnum,
    end_column = diagnostic.end_col,
    severity = severity,
    message = diagnostic.message,
    source = diagnostic.source,
    code = diagnostic.code,
  })
end
return output
";

const QUICKFIX_GET_LUA: &str = r#"
local output = {}
for _, item in ipairs(vim.fn.getqflist()) do
  table.insert(output, {
    path = item.bufnr > 0 and vim.api.nvim_buf_get_name(item.bufnr) or item.filename,
    row = item.lnum > 0 and item.lnum - 1 or nil,
    column = item.col > 0 and item.col - 1 or nil,
    end_row = item.end_lnum > 0 and item.end_lnum - 1 or nil,
    end_column = item.end_col > 0 and item.end_col - 1 or nil,
    text = item.text,
    type = item.type ~= "" and item.type or nil,
    valid = item.valid == 1,
  })
end
return output
"#;
