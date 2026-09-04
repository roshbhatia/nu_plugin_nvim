use std::path::{Path, PathBuf};
use std::time::Duration;

use nu_plugin::{EngineInterface, EvaluatedCall, PluginCommand};
use nu_protocol::{
    Category, IntoSpanned, LabeledError, PipelineData, Record, Signature, Span, SyntaxShape, Type,
    Value,
};
use nuvim_protocol::{
    HandleKind, NvimHandle, QuickfixItem, RpcClient, RpcError, ServerAddress, api_function,
    discover_server, discover_servers,
};
use rmpv::Value as RpcValue;

use crate::NuvimPlugin;
use crate::value::{msgpack_to_nu, nu_to_msgpack};

#[derive(Clone, Copy)]
enum CommandKind {
    Root,
    Servers,
    Context,
    Cursor,
    CursorSet,
    Buffers,
    BufferUse,
    Text,
    Selection,
    Open,
    Edit,
    Replace,
    Diagnostics,
    QuickfixGet,
    QuickfixSet,
    QuickfixOpen,
    Scratch,
    Command,
    Call,
    Lua,
}

struct NuvimCommand(CommandKind);

pub fn all() -> Vec<Box<dyn PluginCommand<Plugin = NuvimPlugin>>> {
    use CommandKind::{
        BufferUse, Buffers, Call, Command, Context, Cursor, CursorSet, Diagnostics, Edit, Lua,
        Open, QuickfixGet, QuickfixOpen, QuickfixSet, Replace, Root, Scratch, Selection, Servers,
        Text,
    };
    [
        Root,
        Servers,
        Context,
        Cursor,
        CursorSet,
        Buffers,
        BufferUse,
        Text,
        Selection,
        Open,
        Edit,
        Replace,
        Diagnostics,
        QuickfixGet,
        QuickfixSet,
        QuickfixOpen,
        Scratch,
        Command,
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
            CommandKind::Servers => "nuvim servers",
            CommandKind::Context => "nuvim context",
            CommandKind::Cursor => "nuvim cursor",
            CommandKind::CursorSet => "nuvim cursor set",
            CommandKind::Buffers => "nuvim buffers",
            CommandKind::BufferUse => "nuvim buffer use",
            CommandKind::Text => "nuvim text",
            CommandKind::Selection => "nuvim selection",
            CommandKind::Open => "nuvim open",
            CommandKind::Edit => "nuvim edit",
            CommandKind::Replace => "nuvim replace",
            CommandKind::Diagnostics => "nuvim diagnostics",
            CommandKind::QuickfixGet => "nuvim quickfix get",
            CommandKind::QuickfixSet => "nuvim quickfix set",
            CommandKind::QuickfixOpen => "nuvim quickfix open",
            CommandKind::Scratch => "nuvim scratch",
            CommandKind::Command => "nuvim command",
            CommandKind::Call => "nuvim call",
            CommandKind::Lua => "nuvim lua",
        }
    }

    fn signature(&self) -> Signature {
        let signature = Signature::build(self.name()).category(Category::Plugin);
        match self.0 {
            CommandKind::Servers => signature.input_output_type(
                Type::Nothing,
                Type::List(Type::Record(vec![].into()).into()),
            ),
            CommandKind::Root
            | CommandKind::Context
            | CommandKind::Cursor
            | CommandKind::Selection => {
                server_flag(signature).input_output_type(Type::Nothing, Type::Record(vec![].into()))
            }
            CommandKind::Buffers | CommandKind::Diagnostics | CommandKind::QuickfixGet => {
                server_flag(signature)
                    .input_output_type(Type::Nothing, Type::List(Type::Any.into()))
            }
            CommandKind::Text => text_signature(signature),
            CommandKind::CursorSet => server_flag(signature)
                .required("row", SyntaxShape::Int, "Zero-based cursor row")
                .required("column", SyntaxShape::Int, "Zero-based cursor byte column")
                .input_output_type(Type::Nothing, Type::Record(vec![].into())),
            CommandKind::BufferUse => server_flag(signature)
                .required("buffer", SyntaxShape::Int, "Buffer ID to make current")
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
            CommandKind::Edit => edit_signature(signature),
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
            CommandKind::Command => server_flag(signature)
                .required("command", SyntaxShape::String, "Ex command to execute")
                .input_output_type(Type::Nothing, Type::Record(vec![].into())),
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
            CommandKind::Root => "Select a running Neovim session",
            CommandKind::Servers => "List running Neovim sessions",
            CommandKind::Context => "Get current Neovim context as a record",
            CommandKind::Cursor => "Get the current zero-based cursor position",
            CommandKind::CursorSet => "Move the cursor to a zero-based position",
            CommandKind::Buffers => "List Neovim buffers as records",
            CommandKind::BufferUse => "Make a loaded buffer current",
            CommandKind::Text => "Read buffer text and its zero-based row range",
            CommandKind::Selection => "Read the last visual selection as structured text",
            CommandKind::Open => "Open paths from pipeline input or arguments",
            CommandKind::Edit => "Replace an exact buffer text range from pipeline input",
            CommandKind::Replace => "Replace a buffer or visual selection with pipeline input",
            CommandKind::Diagnostics => "List Neovim diagnostics as records",
            CommandKind::QuickfixGet => "Get quickfix items with zero-based positions",
            CommandKind::QuickfixSet => "Replace the quickfix list from pipeline records",
            CommandKind::QuickfixOpen => "Open the Neovim quickfix window",
            CommandKind::Scratch => "Open pipeline input in a scratch buffer",
            CommandKind::Command => "Execute an Ex command and return the resulting context",
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
            CommandKind::Root => root(engine, call)?,
            CommandKind::Servers => servers(call.head),
            CommandKind::Context => context(engine, call)?,
            CommandKind::Cursor => cursor(engine, call)?,
            CommandKind::CursorSet => cursor_set(engine, call)?,
            CommandKind::Buffers => buffers(engine, call)?,
            CommandKind::BufferUse => buffer_use(engine, call)?,
            CommandKind::Text => text(engine, call)?,
            CommandKind::Selection => selection(engine, call)?,
            CommandKind::Open => open(engine, call, input)?,
            CommandKind::Edit => edit(engine, call, input)?,
            CommandKind::Replace => replace(engine, call, input)?,
            CommandKind::Diagnostics => diagnostics(engine, call)?,
            CommandKind::QuickfixGet => quickfix_get(engine, call)?,
            CommandKind::QuickfixSet => quickfix_set(engine, call, input)?,
            CommandKind::QuickfixOpen => return quickfix_open(engine, call),
            CommandKind::Scratch => scratch(engine, call, input)?,
            CommandKind::Command => command(engine, call)?,
            CommandKind::Call => raw_call(engine, call, input)?,
            CommandKind::Lua => lua(engine, call, input)?,
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

fn edit_signature(signature: Signature) -> Signature {
    server_flag(signature)
        .required("row", SyntaxShape::Int, "Zero-based start row")
        .required("column", SyntaxShape::Int, "Zero-based start byte column")
        .named(
            "end-row",
            SyntaxShape::Int,
            "Zero-based exclusive end row",
            None,
        )
        .named(
            "end-column",
            SyntaxShape::Int,
            "Zero-based exclusive end byte column",
            None,
        )
        .named(
            "buffer",
            SyntaxShape::Int,
            "Buffer ID; defaults to the current buffer",
            Some('b'),
        )
        .input_output_type(Type::Any, Type::Record(vec![].into()))
}

fn text_signature(signature: Signature) -> Signature {
    server_flag(signature)
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
        .input_output_type(Type::Nothing, Type::Record(vec![].into()))
}

fn servers(span: Span) -> Value {
    let rows = discover_servers()
        .into_iter()
        .filter_map(|address| server_record(&address, span).ok())
        .collect();
    Value::list(rows, span)
}

fn root(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    if let Some(address) = requested_server(engine, call)? {
        return server_record(&address, span);
    }
    let rows = discover_servers()
        .into_iter()
        .filter_map(|address| server_record(&address, span).ok())
        .collect::<Vec<_>>();
    match rows.as_slice() {
        [] => Err(labeled(
            "no live Neovim server found; start Neovim with --listen or set $NVIM",
            span,
        )),
        [row] => Ok(row.clone()),
        _ => pick_server(engine, rows, span),
    }
}

fn pick_server(
    engine: &EngineInterface,
    rows: Vec<Value>,
    span: Span,
) -> Result<Value, LabeledError> {
    let declaration = engine
        .find_decl("input list")
        .map_err(LabeledError::from)?
        .ok_or_else(|| labeled("Nushell command `input list` is unavailable", span))?;
    let picker_call = EvaluatedCall::new(span)
        .with_positional(Value::string("Select a Neovim session", span))
        .with_flag("fuzzy".into_spanned(span))
        .with_flag("per-column".into_spanned(span));
    let selected = engine
        .call_decl(
            declaration,
            picker_call,
            PipelineData::value(Value::list(rows, span), None),
            false,
            false,
        )
        .map_err(LabeledError::from)?
        .into_value(span)
        .map_err(LabeledError::from)?;
    match selected {
        Value::Nothing { .. } => Err(labeled("Neovim session selection was cancelled", span)),
        selected => Ok(selected),
    }
}

fn server_record(
    address: &nuvim_protocol::ServerAddress,
    span: Span,
) -> Result<Value, LabeledError> {
    let mut client = RpcClient::connect_with_timeout(address, Duration::from_millis(250))
        .map_err(|error| labeled(error, span))?;
    let buffer = rpc(client.nvim_get_current_buf(), span)?;
    let path = rpc(client.nvim_buf_get_name([buffer]), span)?;
    let cwd = rpc(
        client.nvim_call_function([RpcValue::from("getcwd"), RpcValue::Array(vec![])]),
        span,
    )?;
    let pid = rpc(
        client.nvim_call_function([RpcValue::from("getpid"), RpcValue::Array(vec![])]),
        span,
    )?;
    let mode = rpc(client.nvim_get_mode(), span)?;
    let path = rpc_string(&path, "buffer name")?;
    let cwd = rpc_string(&cwd, "working directory")?;
    let label = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            Path::new(cwd)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Neovim".to_owned());
    Ok(record(
        [
            ("label", Value::string(label, span)),
            ("server", Value::string(client.server(), span)),
            ("pid", Value::int(rpc_i64(&pid, "process ID", span)?, span)),
            ("cwd", Value::string(cwd, span)),
            ("path", Value::string(path, span)),
            (
                "mode",
                Value::string(rpc_string_field(&mode, "mode")?, span),
            ),
        ],
        span,
    ))
}

fn connect(engine: &EngineInterface, call: &EvaluatedCall) -> Result<RpcClient, LabeledError> {
    let address = requested_server(engine, call)?
        .map_or_else(|| discover_server(None), Ok)
        .map_err(|error| labeled(error, call.head))?;
    RpcClient::connect(&address).map_err(|error| labeled(error, call.head))
}

fn requested_server(
    engine: &EngineInterface,
    call: &EvaluatedCall,
) -> Result<Option<ServerAddress>, LabeledError> {
    let override_value = call
        .get_flag::<String>("server")
        .map_err(LabeledError::from)?;
    let engine_value = if override_value.is_none() {
        match engine.get_env_var("NVIM").map_err(LabeledError::from)? {
            Some(value) => Some(value.as_str().map_err(LabeledError::from)?.to_owned()),
            None => None,
        }
    } else {
        None
    };
    override_value
        .or(engine_value)
        .map(ServerAddress::parse)
        .transpose()
        .map_err(|error| labeled(error, call.head))
}

fn context(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
    context_for_client(&mut client, span)
}

fn context_for_client(client: &mut RpcClient, span: Span) -> Result<Value, LabeledError> {
    let buffer = rpc(client.nvim_get_current_buf(), span)?;
    let window = rpc(client.nvim_get_current_win(), span)?;
    let tab = rpc(client.nvim_get_current_tabpage(), span)?;
    let mode = rpc(client.nvim_get_mode(), span)?;
    let cursor = rpc(client.nvim_win_get_cursor([window.clone()]), span)?;
    let cwd = rpc(
        client.nvim_call_function([RpcValue::from("getcwd"), RpcValue::Array(vec![])]),
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
            ("buffer", buffer_record(client, &buffer, span)?),
            ("window", window_record(client, &window, span)?),
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

fn cursor(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
    current_cursor(&mut client, span)
}

fn current_cursor(client: &mut RpcClient, span: Span) -> Result<Value, LabeledError> {
    let window = rpc(client.nvim_get_current_win(), span)?;
    let position = rpc(client.nvim_win_get_cursor([window]), span)?;
    cursor_record(&position, span)
}

fn cursor_set(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let row = non_negative(
        call.req::<i64>(0).map_err(LabeledError::from)?,
        "cursor row",
        span,
    )?;
    let column = non_negative(
        call.req::<i64>(1).map_err(LabeledError::from)?,
        "cursor column",
        span,
    )?;
    let nvim_row = row
        .checked_add(1)
        .ok_or_else(|| labeled("cursor row is too large", span))?;
    let mut client = connect(engine, call)?;
    let window = rpc(client.nvim_get_current_win(), span)?;
    rpc(
        client.nvim_win_set_cursor([
            window,
            RpcValue::Array(vec![RpcValue::from(nvim_row), RpcValue::from(column)]),
        ]),
        span,
    )?;
    current_cursor(&mut client, span)
}

fn buffers(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
    let listed = rpc(client.nvim_list_bufs(), span)?;
    let listed = rpc_array(&listed, "nvim_list_bufs result", span)?;
    let rows = listed
        .iter()
        .map(|buffer| buffer_record(&mut client, buffer, span))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::list(rows, span))
}

fn buffer_use(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let id = non_negative(
        call.req::<i64>(0).map_err(LabeledError::from)?,
        "buffer ID",
        span,
    )?;
    let id = u64::try_from(id).map_err(|error| labeled(error, span))?;
    let buffer = NvimHandle::new(HandleKind::Buffer, id)
        .to_rpc_value()
        .map_err(|error| labeled(error, span))?;
    let mut client = connect(engine, call)?;
    rpc(client.nvim_set_current_buf([buffer.clone()]), span)?;
    buffer_record(&mut client, &buffer, span)
}

fn text(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
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
        client.nvim_buf_get_lines([
            buffer.clone(),
            RpcValue::from(start),
            RpcValue::from(end),
            RpcValue::Boolean(true),
        ]),
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

fn selection(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
    let result = rpc(
        client.nvim_exec_lua([RpcValue::from(SELECTION_LUA), RpcValue::Array(vec![])]),
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn open(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut paths = paths_from_input(input.into_value(span).map_err(LabeledError::from)?, span)?;
    paths.extend(call.rest::<PathBuf>(0).map_err(LabeledError::from)?);
    if paths.is_empty() {
        return Err(labeled(
            "nuvim open needs path arguments or pipeline input",
            span,
        ));
    }
    let mut client = connect(engine, call)?;
    let mut opened = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.to_string_lossy().into_owned();
        let id = rpc(
            client.nvim_call_function([
                RpcValue::from("bufadd"),
                RpcValue::Array(vec![RpcValue::from(path)]),
            ]),
            span,
        )?
        .as_u64()
        .ok_or_else(|| labeled("bufadd returned a non-integer buffer ID", span))?;
        rpc(
            client.nvim_call_function([
                RpcValue::from("bufload"),
                RpcValue::Array(vec![RpcValue::from(id)]),
            ]),
            span,
        )?;
        let buffer = NvimHandle::new(HandleKind::Buffer, id)
            .to_rpc_value()
            .map_err(|error| labeled(error, span))?;
        if opened.is_empty() {
            rpc(client.nvim_set_current_buf([buffer.clone()]), span)?;
        }
        opened.push(buffer_record(&mut client, &buffer, span)?);
    }
    Ok(Value::list(opened, span))
}

fn edit(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
    let span = call.head;
    let start_row = non_negative(
        call.req::<i64>(0).map_err(LabeledError::from)?,
        "start row",
        span,
    )?;
    let start_column = non_negative(
        call.req::<i64>(1).map_err(LabeledError::from)?,
        "start column",
        span,
    )?;
    let end_row = non_negative(
        call.get_flag::<i64>("end-row")
            .map_err(LabeledError::from)?
            .unwrap_or(start_row),
        "end row",
        span,
    )?;
    let end_column = non_negative(
        call.get_flag::<i64>("end-column")
            .map_err(LabeledError::from)?
            .unwrap_or(start_column),
        "end column",
        span,
    )?;
    if (end_row, end_column) < (start_row, start_column) {
        return Err(labeled("edit end must not precede its start", span));
    }
    let value = input.into_value(span).map_err(LabeledError::from)?;
    let lines = value_to_lines(&value, None, span)?;
    let inserted_lines = i64::try_from(lines.len()).map_err(|error| labeled(error, span))?;
    let mut client = connect(engine, call)?;
    let buffer = selected_buffer(&mut client, call, span)?;
    rpc(
        client.nvim_buf_set_text([
            buffer.clone(),
            RpcValue::from(start_row),
            RpcValue::from(start_column),
            RpcValue::from(end_row),
            RpcValue::from(end_column),
            RpcValue::Array(lines.into_iter().map(RpcValue::from).collect()),
        ]),
        span,
    )?;
    Ok(record(
        [
            ("buffer", buffer_record(&mut client, &buffer, span)?),
            ("start", position_record(start_row, start_column, span)),
            ("end", position_record(end_row, end_column, span)),
            ("inserted_lines", Value::int(inserted_lines, span)),
        ],
        span,
    ))
}

fn replace(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
    let span = call.head;
    let value = input.into_value(span).map_err(LabeledError::from)?;
    let lines = value_to_lines(&value, None, span)?;
    let rpc_lines = RpcValue::Array(
        lines
            .iter()
            .map(|line| RpcValue::from(line.clone()))
            .collect(),
    );
    let mut client = connect(engine, call)?;
    let selection = call.has_flag("selection").map_err(LabeledError::from)?;
    let buffer = selected_buffer(&mut client, call, span)?;
    if selection {
        rpc(
            client.nvim_exec_lua([
                RpcValue::from(REPLACE_SELECTION_LUA),
                RpcValue::Array(vec![rpc_lines]),
            ]),
            span,
        )?;
    } else {
        rpc(
            client.nvim_buf_set_lines([
                buffer.clone(),
                RpcValue::from(0),
                RpcValue::from(-1),
                RpcValue::Boolean(true),
                rpc_lines,
            ]),
            span,
        )?;
    }
    text_for_buffer(&mut client, &buffer, 0, -1, span)
}

fn diagnostics(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
    let result = rpc(
        client.nvim_exec_lua([RpcValue::from(DIAGNOSTICS_LUA), RpcValue::Array(vec![])]),
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn quickfix_get(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let mut client = connect(engine, call)?;
    let result = rpc(
        client.nvim_exec_lua([RpcValue::from(QUICKFIX_GET_LUA), RpcValue::Array(vec![])]),
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn quickfix_set(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
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
    let mut client = connect(engine, call)?;
    rpc(
        client.nvim_call_function([
            RpcValue::from("setqflist"),
            RpcValue::Array(vec![
                RpcValue::Array(vec![]),
                RpcValue::from("r"),
                RpcValue::Map(vec![
                    (RpcValue::from("title"), RpcValue::from(title.clone())),
                    (RpcValue::from("items"), RpcValue::Array(items)),
                ]),
            ]),
        ]),
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

fn quickfix_open(
    engine: &EngineInterface,
    call: &EvaluatedCall,
) -> Result<PipelineData, LabeledError> {
    let span = call.head;
    let height = call.get_flag::<i64>("height").map_err(LabeledError::from)?;
    if height.is_some_and(|height| height <= 0) {
        return Err(labeled("quickfix height must be greater than zero", span));
    }
    let command = height.map_or_else(|| "copen".into(), |height| format!("{height}copen"));
    let mut client = connect(engine, call)?;
    rpc(client.nvim_command([RpcValue::from(command)]), span)?;
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
    let mut client = connect(engine, call)?;
    let buffer = rpc(
        client.nvim_create_buf([RpcValue::Boolean(false), RpcValue::Boolean(true)]),
        span,
    )?;
    rpc(
        client.nvim_buf_set_lines([
            buffer.clone(),
            RpcValue::from(0),
            RpcValue::from(-1),
            RpcValue::Boolean(true),
            RpcValue::Array(lines.into_iter().map(RpcValue::from).collect()),
        ]),
        span,
    )?;
    if let Some(name) = call
        .get_flag::<String>("name")
        .map_err(LabeledError::from)?
    {
        rpc(
            client.nvim_buf_set_name([buffer.clone(), RpcValue::from(name)]),
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
    rpc(client.nvim_set_current_buf([buffer.clone()]), span)?;
    buffer_record(&mut client, &buffer, span)
}

fn command(engine: &EngineInterface, call: &EvaluatedCall) -> Result<Value, LabeledError> {
    let span = call.head;
    let command = call.req::<String>(0).map_err(LabeledError::from)?;
    let mut client = connect(engine, call)?;
    rpc(client.nvim_command([RpcValue::from(command)]), span)?;
    context_for_client(&mut client, span)
}

fn raw_call(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
    let span = call.head;
    let method = call.req::<String>(0).map_err(LabeledError::from)?;
    let mut arguments = call.rest::<Value>(1).map_err(LabeledError::from)?;
    append_pipeline_arguments(
        &mut arguments,
        input.into_value(span).map_err(LabeledError::from)?,
    );
    let function = api_function(&method).ok_or_else(|| {
        labeled(
            format!("generated Neovim API metadata does not contain method {method}"),
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
    let mut client = connect(engine, call)?;
    let arguments = arguments
        .iter()
        .map(|value| nu_to_msgpack(value, client.server()))
        .collect::<Result<Vec<_>, _>>()?;
    let result = rpc(client.call(&method, arguments), span)?;
    msgpack_to_nu(&result, client.server(), span)
}

fn lua(
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> Result<Value, LabeledError> {
    let span = call.head;
    let code = call.req::<String>(0).map_err(LabeledError::from)?;
    let mut arguments = call.rest::<Value>(1).map_err(LabeledError::from)?;
    append_pipeline_arguments(
        &mut arguments,
        input.into_value(span).map_err(LabeledError::from)?,
    );
    let mut client = connect(engine, call)?;
    let arguments = arguments
        .iter()
        .map(|value| nu_to_msgpack(value, client.server()))
        .collect::<Result<Vec<_>, _>>()?;
    let result = rpc(
        client.nvim_exec_lua([RpcValue::from(code), RpcValue::Array(arguments)]),
        span,
    )?;
    msgpack_to_nu(&result, client.server(), span)
}

fn rpc(result: Result<RpcValue, RpcError>, span: Span) -> Result<RpcValue, LabeledError> {
    result.map_err(|error| labeled(error, span))
}

fn buffer_record(
    client: &mut RpcClient,
    buffer: &RpcValue,
    span: Span,
) -> Result<Value, LabeledError> {
    let name = rpc(client.nvim_buf_get_name([buffer.clone()]), span)?;
    let changedtick = rpc(client.nvim_buf_get_changedtick([buffer.clone()]), span)?;
    let loaded = rpc(client.nvim_buf_is_loaded([buffer.clone()]), span)?;
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
    let width = rpc(client.nvim_win_get_width([window.clone()]), span)?;
    let height = rpc(client.nvim_win_get_height([window.clone()]), span)?;
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
    Ok(position_record(row.saturating_sub(1), column, span))
}

fn position_record(row: i64, column: i64, span: Span) -> Value {
    record(
        [
            ("row", Value::int(row, span)),
            ("column", Value::int(column, span)),
        ],
        span,
    )
}

fn non_negative(value: i64, name: &str, span: Span) -> Result<i64, LabeledError> {
    if value < 0 {
        return Err(labeled(format!("{name} must be zero or greater"), span));
    }
    Ok(value)
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
    rpc(client.nvim_get_current_buf(), span)
}

fn get_buffer_option(
    client: &mut RpcClient,
    buffer: &RpcValue,
    name: &str,
    span: Span,
) -> Result<RpcValue, LabeledError> {
    rpc(
        client.nvim_get_option_value([
            RpcValue::from(name),
            RpcValue::Map(vec![(RpcValue::from("buf"), buffer.clone())]),
        ]),
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
        client.nvim_set_option_value([
            RpcValue::from(name),
            value,
            RpcValue::Map(vec![(RpcValue::from("buf"), buffer.clone())]),
        ]),
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
