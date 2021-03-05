use anyhow::Result;
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessHandler,
    ProcessScope,
};

struct Ports {
    x: Port<AudioOut>,
    y: Port<AudioOut>,
    z: Port<AudioOut>,
}

impl Ports {
    pub fn new(client: &Client) -> Result<Self, jack::Error> {
        Ok(Self {
            x: client.register_port("x_out", AudioOut::default())?,
            y: client.register_port("y_out", AudioOut::default())?,
            z: client.register_port("z_out", AudioOut::default())?,
        })
    }
}

struct RTProcess {
    ports: Ports,
    sample_rate: usize,
    frame_t: f64,
    t: f64,
}

impl ProcessHandler for RTProcess {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        // todo!()
        let x_out = self.ports.x.as_mut_slice(ps);

        for v in x_out.iter_mut() {
            let x = 2.0*std::f64::consts::PI*self.t;
            let y = x.sin();
            *v = y as f32;
            self.t += self.frame_t;
        }

        Control::Continue
    }
}

struct NotifProcess {}
impl NotificationHandler for NotifProcess {
    fn xrun(&mut self, _client: &Client) -> Control {
        println!("xrun!");

        Control::Continue
    }
}

fn main() -> Result<()> {
    let (client, _status) = Client::new("scopefig", ClientOptions::NO_START_SERVER)?;

    let process_handler = RTProcess {
        ports: Ports::new(&client)?,
        sample_rate: client.sample_rate(),
        frame_t: 1.0 / client.sample_rate() as f64,
        t: 0.0f64,
    };
    let notif_handler = NotifProcess {};
    client.activate_async(notif_handler, process_handler)?;

    loop {} // spin

    // client.de();

    Ok(())
}
