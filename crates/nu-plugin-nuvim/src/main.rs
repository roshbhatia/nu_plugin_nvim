use nu_plugin::{MsgPackSerializer, serve_plugin};
use nu_plugin_nuvim::NuvimPlugin;

fn main() {
    serve_plugin(&NuvimPlugin, MsgPackSerializer);
}
