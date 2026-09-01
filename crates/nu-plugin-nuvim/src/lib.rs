mod commands;
pub mod value;

use nu_plugin::{Plugin, PluginCommand};

pub struct NuvimPlugin;

impl Plugin for NuvimPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        commands::all()
    }
}
