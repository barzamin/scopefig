use std::{
    fs::{self, File},
    io::{BufWriter, Seek, Write},
    path::PathBuf,
};

use anyhow::{anyhow, Result};
use hound::WavWriter;
use kv_log_macro as log;
use lyon_geom::{LineSegment, Point};
use lyon_path::Path;
use lyon_path::{iterator::Flattened, PathEvent};
use plotters::{coord::types::RangedCoordf32, prelude::*};
use structopt::StructOpt;
use usvg::{prelude::*, Align, FitTo, Size, Transform, ViewBox};

mod svg;

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(short, long, parse(from_os_str), default_value = "out.wav")]
    output: PathBuf,
}

const F_s: u32 = 44_100; // Hz
const DRAW_DWELL: f32 = 0.01; // s/unit
const JUMP_TIME: f32 = 0.0005; // sec per jump
const TOLERANCE: f32 = 0.01;

fn draw_line(pts: &mut Vec<Point<f32>>, line: LineSegment<f32>) {
    // log::debug!("emit line {:?}", line);

    let n_samples: usize = ((F_s as f32 * line.length() * DRAW_DWELL).trunc() as usize).max(1);
    // log::debug!("  drawing line with n_samples: {}", n_samples);
    for t in (0..n_samples).map(|i| i as f32 / n_samples as f32) {
        // log::trace!("    generate pt t : {}, sample : {:?}", t, line.sample(t));
        pts.push(line.sample(t));
    }
}

fn jump_easing(k: i32, x: f32) -> f32 {
    if x <= 0.5 {
        (1./(0.5f32).powi(k-1)) * x.powi(k)
    } else {
        1. - (-2. * x + 2.).powi(k) / 2.
    }
}

fn jump(pts: &mut Vec<Point<f32>>, from: Option<Point<f32>>, to: Point<f32>) {
    log::debug!("emit jump {:?} -> {:?}", from, to);

    if let Some(from) = from {
        let line = LineSegment { from, to };
        let n_samples: usize = (F_s as f32 * line.length() * JUMP_TIME).trunc() as usize;
        log::debug!("  jump with n_samples: {}", n_samples);
        for t in (0..n_samples).map(|i| i as f32 / n_samples as f32).map(|t| jump_easing(10, t)) {
            log::trace!("    generate pt t : {}, sample : {:?}", t, line.sample(t));
            pts.push(line.sample(t));
        }
    } else {
        pts.push(to);
    }
}

fn write_wav<W>(wtr: W, pts: &[Point<f32>]) -> Result<(), hound::Error>
where
    W: Write + Seek,
{
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: F_s,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut wav_wtr = hound::WavWriter::new(wtr, spec)?;
    for _ in 0..10 {
        for pt in pts {
            wav_wtr.write_sample(pt.x)?;
            wav_wtr.write_sample(pt.y)?;
        }
    }
    wav_wtr.finalize()?;

    Ok(())
}

fn transform(
    base_txform: usvg::Transform,
    view_box: ViewBox,
    size: usvg::Size,
) -> lyon_geom::Transform<f32> {
    let scale = (2. / view_box.rect.width().max(view_box.rect.height())) as f32;
    log::debug!("scale: {}", scale);

    lyon_geom::Transform::<f32>::new(
        base_txform.a as f32,
        base_txform.b as f32,
        base_txform.c as f32,
        base_txform.d as f32,
        base_txform.e as f32,
        base_txform.f as f32,
    )
    .then_translate(lyon_geom::Vector::new(
        (-view_box.rect.width() / 2.) as f32,
        (-view_box.rect.height() / 2.) as f32,
    ))
    .then_scale(scale, -scale)
}

fn main() -> Result<()> {
    femme::with_level(femme::LevelFilter::Trace);

    let args = Args::from_args();

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_file(args.input, &opt)?;

    let mut pts: Vec<Point<f32>> = vec![/*Point::new(0., 0.)*/];

    let view_box = tree.svg_node().view_box;
    let size = tree.svg_node().size;
    log::info!("view_box: {:?}, size: {:?}", view_box, size);
    log::info!("{:?}", size.fit_view_box(&view_box));

    for node in tree.root().descendants() {
        if let usvg::NodeKind::Path(ref p) = *node.borrow() {
            log::debug!("handling node {:?}", node);
            let mut txform = transform(node.transform(), view_box, size);
            println!("txform: {:?}", txform);
            let flattened = Flattened::new(TOLERANCE, svg::convert_path(p));
            for evt in flattened {
                log::trace!(" -> {:?}", evt);
                match evt {
                    lyon_path::Event::Begin { at } => {
                        let last = pts.last().cloned();
                        jump(&mut pts, last, txform.transform_point(at));
                    }
                    lyon_path::Event::End { last, first, close } => {
                        if close {
                            let line = LineSegment {
                                from: txform.transform_point(last),
                                to: txform.transform_point(first),
                            };
                            println!("--> CLOSING");
                            draw_line(&mut pts, line);
                        }
                    }
                    lyon_path::Event::Line { from, to } => {
                        let line = LineSegment {
                            from: txform.transform_point(from),
                            to: txform.transform_point(to),
                        };
                        draw_line(&mut pts, line);
                    }
                    _ => {
                        log::warn!("unsupported path element {:?}", evt);
                    }
                }
            }
        }
    }

    let last = pts.last().cloned();
    jump(&mut pts, last, Point::new(0., 0.0));

    let mut root = BitMapBackend::new("plot.png", (600, 600)).into_drawing_area();
    root.fill(&WHITE)?;
    let root = root.apply_coord_spec(Cartesian2d::<RangedCoordf32, RangedCoordf32>::new(
        -1f32..1f32,
        -1f32..1f32,
        // min_x..max_x,
        // min_y..max_y,
        (0..600, 0..600),
    ));

    for pt in &pts {
        root.draw(&(EmptyElement::at((pt.x, -pt.y)) + Circle::new((0, 0), 3, ShapeStyle::from(&BLACK).filled())))?;
    }


    write_wav(BufWriter::new(File::create(args.output)?), &pts)?;

    Ok(())
}
