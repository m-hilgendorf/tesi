use tesi_graph as graph;

#[derive(Copy, Clone, Debug, Default)]
pub struct Sine {
    phase: f32,
    freq: f32,
    sample_rate: f32,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Sum;

impl Sine {
    pub fn new(freq: f32) -> Self {
        Self {
            phase: 0.0,
            freq,
            sample_rate: 48e3,
        }
    }
}

impl graph::proc::Processor for Sine {
    fn initialize(&mut self, sample_rate: f64, _max_num_frames: usize) {
        self.sample_rate = sample_rate as f32;
    }

    fn process(&mut self, context: &mut graph::proc::Context<'_>) {
        let output = &mut context.audio_outputs[0];
        for channel in 0..output.num_channels() {
            let channel = output[channel].as_ptr();
            eprintln!("writing to {channel:x?}");
        }
        for sample in 0..output.num_frames() {
            let sine = (self.phase * std::f32::consts::TAU).sin();
            self.phase = (self.phase + self.freq / self.sample_rate).fract();
            for channel in output.iter() {
                channel[sample] = sine;
            }
        }
    }

    fn reset(&mut self) {
        self.phase = 0.0;
    }
}

impl graph::proc::Processor for Sum {
    fn initialize(&mut self, _sample_rate: f64, _max_num_frames: usize) {}

    fn process(&mut self, context: &mut graph::proc::Context<'_>) {
        let output = &mut context.audio_outputs[0];
        output.clear();
        for input in context.audio_inputs {
            for channel in 0..input.num_channels() {
                let input_channel = input[channel].as_ptr();
                let output_channel = output[channel].as_ptr();
                eprintln!("adding {input_channel:x?} to {output_channel:x?}");
                for (i, o) in input[channel].iter().zip(output[channel].iter_mut()) {
                    *o += *i;
                }
            }
        }
    }

    fn reset(&mut self) {}
}

fn main() {
    let options = graph::graph::Options {
        num_input_channels: 0,
        num_output_channels: 2,
        num_workers: 0,
    };

    let graph = graph::Graph::new(options);

    // Create some sources.
    let options = graph::node::Options {
        audio_inputs: vec![],
        audio_outputs: vec![2],
    };
    let sine440 = graph::node::Node::new(&graph, options.clone(), Sine::new(440.0));
    let sine880 = graph::node::Node::new(&graph, options, Sine::new(880.0));

    // Create a summer.
    let options = graph::node::Options {
        audio_inputs: vec![2, 2],
        audio_outputs: vec![2],
    };
    let sum = graph::node::Node::new(&graph, options, Sum);

    // Connect the graph.
    let _e1 = graph::edge::Edge::new(&graph, &sine440, 0, &sum, 0).unwrap();
    let _e2 = graph::edge::Edge::new(&graph, &sine880, 0, &sum, 1).unwrap();
    let _e3 = graph::edge::Edge::new(&graph, &sum, 0, &graph.output_node(), 0);

    // Compile the changes.
    graph.commit_changes();

    // Get the renderer.
    let mut renderer = graph.renderer().unwrap();

    // Create some i/o data.
    let buffer_size = 128;
    let input = vec![];
    let mut output = vec![0.0; 2 * buffer_size];
    let mut output_ptrs =
        unsafe { vec![output.as_mut_ptr(), output.as_mut_ptr().add(buffer_size)] };

    // Render.
    renderer.initialize(48e3, buffer_size);
    renderer.render(input.as_ptr(), output_ptrs.as_mut_ptr(), 0, 2, buffer_size);

    let (left, right) = output.split_at(buffer_size);
    assert_eq!(left, right);

    println!("l = {left:?};");
    println!("r = {right:?};");
}
