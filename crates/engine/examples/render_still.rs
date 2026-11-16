//! Renders one frame of a project to a PNG, exactly as preview and export draw it.
//! Usage: cargo run -p engine --example render_still -- project.ave FRAME WIDTH HEIGHT out.png

use engine::{Compositor, Project};
use std::path::{Path, PathBuf};

fn main() -> engine::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        eprintln!("usage: render_still project.ave FRAME WIDTH HEIGHT out.png");
        std::process::exit(2);
    }
    let project = Project::load(Path::new(&args[1]))?;
    let fonts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
    let mut comp = Compositor::new(Some(&fonts));
    let frame = comp.render(&project, args[2].parse().unwrap(), args[3].parse().unwrap(), args[4].parse().unwrap())?;
    frame.save_png(Path::new(&args[5]))
}
