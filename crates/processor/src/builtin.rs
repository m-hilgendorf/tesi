pub mod dummy {
    use std::sync::{Arc, Mutex};

    use crate::{Port, context, editor::ParameterTree, processor};

    pub struct Processor {
        ports: Arc<Mutex<Vec<Port>>>,
    }

    pub struct Editor {
        ports: Arc<Mutex<Vec<Port>>>,
    }

    impl crate::Processor for Processor {
        fn activate(&mut self, _: crate::context::Activate) -> Option<crate::processor::Activated> {
            Some(processor::Activated { latency: None })
        }

        fn start(&mut self) -> bool {
            true
        }

        fn stop(&mut self) -> bool {
            true
        }

        fn editor(&self) -> Box<dyn crate::Editor> {
            Box::new(Editor {
                ports: self.ports.clone(),
            })
        }

        fn process(&mut self, _: context::Process<'_>) -> processor::Processed {
            processor::Processed {
                state: processor::State::Continue,
                tail_frames: None,
            }
        }

        fn reset(&mut self) {}
    }

    impl crate::Editor for Editor {
        fn attach_gui(&mut self, _gui: crate::editor::GuiHandle) -> bool {
            false
        }

        fn capabilites(&self) -> crate::editor::Capabilities {
            0
        }

        fn fmt_param(&mut self, _id: u64, _value: f64) -> Option<String> {
            None
        }

        fn get_param(&mut self, _id: u64) -> Option<f64> {
            None
        }

        fn params(&mut self) -> crate::editor::ParameterTree {
            ParameterTree {
                name: "dummy".into(),
                id: None,
                value: None,
                children: Vec::new(),
            }
        }

        fn get_ports(&mut self) -> Vec<Port> {
            self.ports.lock().unwrap().clone()
        }

        fn parse_param(&mut self, _id: u64, _text: &str) -> Option<f64> {
            None
        }

        fn load(&mut self, _bytes: &[u8]) {}

        fn save(&mut self) -> Vec<u8> {
            Vec::new()
        }

        fn set_param(&mut self, _id: u64, _value: f64) {}
        fn set_ports(&mut self, ports: &[Port]) -> bool {
            *self.ports.lock().unwrap() = ports.to_vec();
            true
        }
    }
}
