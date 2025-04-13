use crate::Port;
pub use crate::port;

pub trait Editor {
    /// Returns the (static) capabilities of a node.
    fn capabilites(&self) -> Capabilities;

    /// Save state. Requires [capabilities::PERSISTENT]. Requires [capabilities::THREAD_SAFE_STATE]
    /// in order to be called off the main thread.
    #[allow(unused_variables)]
    fn save(&mut self) -> Vec<u8> {
        Vec::new()
    }

    /// Load state. Requires [capabilities::PERSISTENT]. Requires [capabilities::THREAD_SAFE_STATE]
    /// in order to be called off the main thread.
    #[allow(unused_variables)]
    fn load(&mut self, bytes: &[u8]) {}

    /// Return the list of default ports.
    fn get_ports(&mut self) -> Vec<Port>;

    /// Configure the set of ports, returning if it was successful or not. Requires [capabilities::CONFIGURABLE_PORTS].
    #[allow(unused_variables)]
    fn set_ports(&mut self, ports: &[Port]) -> bool {
        false
    }

    /// Return the parameter tree.
    fn params(&mut self) -> ParameterTree;

    /// Get a parameter value.
    fn get_param(&mut self, id: u64) -> Option<f64>;

    /// Set the parameter value.
    fn set_param(&mut self, id: u64, value: f64);

    /// Format the parameter value as a string. Returns Some on success.
    #[allow(unused_variables)]
    fn fmt_param(&mut self, id: u64, value: f64) -> Option<String> {
        None
    }

    /// Parse the parameter value to get its normalized value. Returns Some on success.
    #[allow(unused_variables)]
    fn parse_param(&mut self, id: u64, text: &str) -> Option<f64> {
        None
    }

    /// Provide a GUI window for the plugin.
    #[allow(unused_variables)]
    fn attach_gui(&mut self, gui: GuiHandle) -> bool {
        false
    }
}

/// A parameter tree holds the parameters in its leaves.
#[derive(Default, Debug)]
pub struct ParameterTree {
    /// The parameter name. Optional for branches.
    pub name: String,

    /// The parameter ID, None if this is a branch.
    pub id: Option<u64>,

    // The parameter value, None if this is a branch.
    pub value: Option<f64>,

    /// The children, empty if this is a leaf.
    pub children: Vec<Self>,
}

pub struct Iter<'a> {
    stack: Vec<&'a ParameterTree>,
}

impl ParameterTree {
    fn iter(&self) -> Iter<'_> {
        Iter { stack: vec![self] }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a ParameterTree;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.stack.pop()?;
        self.stack.extend(next.children.iter());
        Some(next)
    }
}

impl<'a> IntoIterator for &'a ParameterTree {
    type IntoIter = Iter<'a>;
    type Item = Self;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub type Capability = u64;
pub type Capabilities = u64;

pub mod capabilities {
    /// Enable save/load capabilites.
    pub const PERSISTENT: u64 = 1 << 0;

    /// Enable [super::Editor::set_ports] port.
    pub const CONFIGURABLE_PORTS: u64 = 1 << 1;

    /// Enable [crate::Processor::activate] to be called on any thread.
    pub const THREAD_SAFE_ACTIVATE: u64 = 1 << 2;

    /// Enable [super::Editor::load] to be called on any thread.
    pub const THREAD_SAFE_LOAD: u64 = 1 << 2;

    /// Enable [super::Editor::save] to be called on any thread.
    pub const THREAD_SAFE_SAVE: u64 = 1 << 3;

    /// Enable [super::Editor::get_ports] to be called on any thread.
    pub const THREAD_SAFE_GET_PORTS: u64 = 1 << 4;

    /// Enable [super::Editor::set_ports] to be called on any thread.
    pub const THREAD_SAFE_SET_PORTS: u64 = 1 << 5;

    /// Enable [super::Editor::params] to be called on any thread.
    pub const THREAD_SAFE_PARAMS: u64 = 1 << 6;

    /// Enable [super::Editor::get_param] to be called on any thread.
    pub const THREAD_SAFE_GET_PARAM: u64 = 1 << 6;

    /// Enable [super::Editor::set_param] to be called on any thread.
    pub const THREAD_SAFE_SET_PARAM: u64 = 1 << 7;

    /// Enable [super::Editor::fmt_param] to be called on any thread.
    pub const THREAD_SAFE_FMT_PARAM: u64 = 1 << 8;

    /// Enable [super::Editor::parse_param] to be called on any thread.
    pub const THREAD_SAFE_PARSE_PARAM: u64 = 1 << 9;

    /// Enable all THREAD_SAFE_* capabilities.
    pub const THREAD_SAFE: u64 = THREAD_SAFE_ACTIVATE
        | THREAD_SAFE_SAVE
        | THREAD_SAFE_LOAD
        | THREAD_SAFE_GET_PORTS
        | THREAD_SAFE_SET_PORTS
        | THREAD_SAFE_PARAMS
        | THREAD_SAFE_GET_PARAM
        | THREAD_SAFE_SET_PARAM
        | THREAD_SAFE_FMT_PARAM
        | THREAD_SAFE_PARSE_PARAM;
}

pub enum GuiHandle {
    // TODO: raw window handle
}
