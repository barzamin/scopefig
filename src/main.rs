use anyhow::Result;
use itertools::izip;
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessHandler,
    ProcessScope,
};

#[derive(Debug)]
struct Ports {
    x: Port<AudioOut>,
    y: Port<AudioOut>,
    z: Port<AudioOut>,
}

impl Ports {
    pub fn new(client: &Client) -> Result<Self, jack::Error> {
        Ok(Self {
            x: client.register_port("x", AudioOut::default())?,
            y: client.register_port("y", AudioOut::default())?,
            z: client.register_port("z", AudioOut::default())?,
        })
    }
}

#[derive(Debug)]
struct RTProcess {
    ports: Ports,
    sample_rate: usize,
    frame_t: f64,
    t: f64,
}

const SCAN_RATE: f64 = 100.; // 100hz => 2pi rad around the cardioid per 1/100 s.

impl ProcessHandler for RTProcess {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        let x_out = self.ports.x.as_mut_slice(ps);
        let y_out = self.ports.y.as_mut_slice(ps);
        let z_out = self.ports.z.as_mut_slice(ps);

        for (x, y, z) in izip!(x_out.iter_mut(), y_out.iter_mut(), z_out.iter_mut()) {
            let t = self.t * 2. * std::f64::consts::PI * SCAN_RATE;

            let scl = (1.-0.2)-(self.t*2.*std::f64::consts::PI*0.5).cos()*0.2;

            *x = (scl*( 16.*t.sin().powi(3) )/18.) as f32;
            *y = (scl*( 13.*t.cos() - 5.*(2.*t).cos() - 2.*(3.*t).cos() - (4.*t).cos() )/18.) as f32;

            // *z = if (t % 1.) > 0.5 { 1.0 } else {0.0} as f32;
            *z = 0.;

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

    let ports = Ports::new(&client)?;

    let process_handler = RTProcess {
        ports,
        sample_rate: client.sample_rate(),
        frame_t: 1.0 / client.sample_rate() as f64,
        t: 0.0f64,
    };

    let notif_handler = NotifProcess {};
    let active_client = client.activate_async(notif_handler, process_handler)?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
    active_client.deactivate()?;

    Ok(())
}
