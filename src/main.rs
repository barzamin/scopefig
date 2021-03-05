use anyhow::Result;
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessHandler,
    ProcessScope,
};
use itertools::izip;

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
        let x_out = self.ports.x.as_mut_slice(ps);
        let y_out = self.ports.y.as_mut_slice(ps);
        let z_out = self.ports.z.as_mut_slice(ps);

        for (x, y, z) in izip!(x_out.iter_mut(), y_out.iter_mut(), z_out.iter_mut()) {
            *x = 0.0f32;
            *y = 0.0f32;
            *z = 0.0f32;

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
